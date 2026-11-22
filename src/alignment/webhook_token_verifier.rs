//! Constant-time `payload.secret_token` verification and mandatory `raw_redacted` scrubbing
//! (US-9.2.0 Unit 1).
//!
//! Moyasar authenticates webhook deliveries by embedding a shared `secret_token` inside the JSON
//! payload. This module verifies that token in constant time over fixed-length SHA-256 digests
//! (preventing length-leak timing side channels, TM-6), extracts canonical metadata with fail-closed
//! `livemode` semantics (TM-3), and recursively strips every `"secret_token"` key before returning
//! the payload (TM-4).

use serde_json::Value;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::kernel::error::MoyasarError;

/// A verified Moyasar webhook payload whose embedded `secret_token` has been scrubbed.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::webhook_token_verifier::{
///     VerifiedMoyasarWebhook, verify_and_scrub_moyasar_webhook,
/// };
///
/// let body = br#"{
///     "id": "evt_123",
///     "type": "payment_paid",
///     "live": false,
///     "secret_token": "wh_secret_123",
///     "data": { "id": "pay_123" }
/// }"#;
///
/// let verified: VerifiedMoyasarWebhook =
///     verify_and_scrub_moyasar_webhook(body, "wh_secret_123").expect("valid webhook");
/// assert_eq!(verified.event_id, "evt_123");
/// assert_eq!(verified.event_type, "payment_paid");
/// assert!(!verified.livemode);
/// assert!(verified.raw_redacted.get("secret_token").is_none());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedMoyasarWebhook {
    /// Unique Moyasar webhook event identifier extracted from `payload.id`.
    pub event_id: String,
    /// Moyasar webhook event type extracted from `payload.type`.
    pub event_type: String,
    /// Whether the webhook originated from Moyasar's live environment (`payload.live`).
    pub livemode: bool,
    /// Parsed webhook JSON payload with every `"secret_token"` key recursively removed.
    pub raw_redacted: Value,
}

/// Compares two webhook secret tokens in constant time using SHA-256 digests.
///
/// Hashing both inputs to fixed 32-byte digests before comparing with [`ConstantTimeEq::ct_eq`]
/// eliminates length-leak timing side channels (TM-6). Returns `false` if either input is empty.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::webhook_token_verifier::constant_time_token_eq;
///
/// assert!(constant_time_token_eq("wh_secret_123", "wh_secret_123"));
/// assert!(!constant_time_token_eq("wh_secret_123", "wh_secret_456"));
/// assert!(!constant_time_token_eq("", ""));
/// ```
#[must_use]
pub fn constant_time_token_eq(expected: &str, actual: &str) -> bool {
    if expected.is_empty() || actual.is_empty() {
        return false;
    }
    let expected_digest = Sha256::digest(expected.as_bytes());
    let actual_digest = Sha256::digest(actual.as_bytes());
    bool::from(expected_digest.as_slice().ct_eq(actual_digest.as_slice()))
}

/// Recursively removes every `"secret_token"` key from a JSON value.
///
/// Traverses top-level objects, nested objects, and arrays so that `secret_token` can never
/// survive in `raw_redacted` payloads persisted to logs or downstream stores (TM-4).
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// use some_moyasar::alignment::webhook_token_verifier::scrub_secret_token;
///
/// let mut payload = json!({
///     "id": "evt_1",
///     "secret_token": "wh_secret_123",
///     "nested": { "secret_token": "wh_secret_123", "keep": true }
/// });
/// scrub_secret_token(&mut payload);
/// assert!(!payload.to_string().contains("wh_secret_123"));
/// assert_eq!(payload["nested"]["keep"], json!(true));
/// ```
pub fn scrub_secret_token(payload: &mut Value) {
    match payload {
        Value::Object(map) => {
            map.remove("secret_token");
            for value in map.values_mut() {
                scrub_secret_token(value);
            }
        }
        Value::Array(items) => {
            for item in items {
                scrub_secret_token(item);
            }
        }
        _ => {}
    }
}

fn parse_json_object(raw_body: &[u8]) -> Result<Value, MoyasarError> {
    if raw_body.is_empty() {
        return Err(MoyasarError::invalid_request(
            "webhook payload must not be empty",
        ));
    }
    let payload: Value = serde_json::from_slice(raw_body)
        .map_err(|_| MoyasarError::invalid_request("webhook payload must be valid JSON"))?;
    if !payload.is_object() {
        return Err(MoyasarError::invalid_request(
            "webhook payload must be a JSON object",
        ));
    }
    Ok(payload)
}

fn verify_secret_token(payload: &Value, expected_secret: &str) -> Result<(), MoyasarError> {
    let provided_token = payload
        .get("secret_token")
        .and_then(Value::as_str)
        .ok_or(MoyasarError::InvalidWebhookSecret)?;
    if !constant_time_token_eq(expected_secret, provided_token) {
        return Err(MoyasarError::InvalidWebhookSecret);
    }
    Ok(())
}

fn extract_required_str(payload: &Value, field: &str) -> Result<String, MoyasarError> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            MoyasarError::invalid_request(format!("missing or empty webhook field `{field}`"))
        })
}

/// Verifies a raw Moyasar webhook payload's `secret_token` and returns a scrubbed envelope.
///
/// Performs constant-time verification against `expected_secret` (failing closed without leaking
/// any secret material in error messages), extracts `id`, `type`, and `live` (defaulting to
/// `false` if absent so test webhooks never escalate to live mode, TM-3), and recursively removes
/// all `"secret_token"` keys before constructing [`VerifiedMoyasarWebhook`] (TM-4).
///
/// # Errors
///
/// Returns [`MoyasarError::InvalidWebhookSecret`] when `expected_secret` is empty, `"secret_token"`
/// is missing or non-string, or the token does not match `expected_secret`.
/// Returns [`MoyasarError::Api`] with [`crate::kernel::error::ApiErrorType::InvalidRequestError`]
/// when `raw_body` is empty, invalid JSON, not a JSON object, or missing non-empty `"id"` /
/// `"type"` strings.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::webhook_token_verifier::verify_and_scrub_moyasar_webhook;
///
/// let payload = br#"{
///     "id": "evt_live_42",
///     "type": "payment_paid",
///     "live": true,
///     "secret_token": "wh_secret_123"
/// }"#;
///
/// let verified =
///     verify_and_scrub_moyasar_webhook(payload, "wh_secret_123").expect("verified webhook");
/// assert_eq!(verified.event_id, "evt_live_42");
/// assert_eq!(verified.event_type, "payment_paid");
/// assert!(verified.livemode);
/// assert!(!verified.raw_redacted.to_string().contains("wh_secret_123"));
/// ```
pub fn verify_and_scrub_moyasar_webhook(
    raw_body: &[u8],
    expected_secret: &str,
) -> Result<VerifiedMoyasarWebhook, MoyasarError> {
    if expected_secret.is_empty() {
        return Err(MoyasarError::InvalidWebhookSecret);
    }
    let mut payload = parse_json_object(raw_body)?;
    verify_secret_token(&payload, expected_secret)?;
    let event_id = extract_required_str(&payload, "id")?;
    let event_type = extract_required_str(&payload, "type")?;
    let livemode = payload
        .get("live")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    scrub_secret_token(&mut payload);
    Ok(VerifiedMoyasarWebhook {
        event_id,
        event_type,
        livemode,
        raw_redacted: payload,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const CANARY_SECRET: &str = "wh_secret_123";

    #[test]
    fn tc_9_2_1_constant_time_token_eq_handles_matches_mismatches_and_empty() {
        assert!(constant_time_token_eq(CANARY_SECRET, CANARY_SECRET));
        assert!(!constant_time_token_eq(CANARY_SECRET, "wh_secret_124"));
        assert!(!constant_time_token_eq(CANARY_SECRET, "short"));
        assert!(!constant_time_token_eq("", CANARY_SECRET));
        assert!(!constant_time_token_eq(CANARY_SECRET, ""));
        assert!(!constant_time_token_eq("", ""));
    }

    #[test]
    fn tc_9_2_1_valid_secret_token_passes_and_scrubs_top_level_and_nested_canaries() {
        let body = json!({
            "id": "evt_001",
            "type": "payment_paid",
            "live": true,
            "secret_token": CANARY_SECRET,
            "data": {
                "id": "pay_001",
                "secret_token": CANARY_SECRET,
                "nested_list": [
                    { "secret_token": CANARY_SECRET, "amount": 1000 }
                ]
            }
        });
        let raw = serde_json::to_vec(&body).expect("serialize payload");
        let verified =
            verify_and_scrub_moyasar_webhook(&raw, CANARY_SECRET).expect("valid webhook");

        assert_eq!(verified.event_id, "evt_001");
        assert_eq!(verified.event_type, "payment_paid");
        assert!(verified.livemode);

        let serialized = verified.raw_redacted.to_string();
        assert!(!serialized.contains(CANARY_SECRET));
        assert!(!serialized.contains("secret_token"));
        assert_eq!(verified.raw_redacted["data"]["id"], json!("pay_001"));
        assert_eq!(
            verified.raw_redacted["data"]["nested_list"][0]["amount"],
            json!(1000)
        );
    }

    #[test]
    fn tc_9_2_1_live_false_and_missing_live_default_to_test_mode() {
        let live_false = br#"{
            "id": "evt_test_1",
            "type": "payment_failed",
            "live": false,
            "secret_token": "wh_secret_123"
        }"#;
        let verified_false =
            verify_and_scrub_moyasar_webhook(live_false, CANARY_SECRET).expect("live=false");
        assert!(!verified_false.livemode);

        let missing_live = br#"{
            "id": "evt_test_2",
            "type": "payment_failed",
            "secret_token": "wh_secret_123"
        }"#;
        let verified_missing =
            verify_and_scrub_moyasar_webhook(missing_live, CANARY_SECRET).expect("missing live");
        assert!(!verified_missing.livemode);
    }

    #[test]
    fn tc_9_2_1_wrong_secret_token_rejected_without_leaking_secret() {
        let body = br#"{
            "id": "evt_002",
            "type": "payment_paid",
            "live": true,
            "secret_token": "wrong_secret_xyz"
        }"#;
        let err = verify_and_scrub_moyasar_webhook(body, CANARY_SECRET)
            .expect_err("wrong secret must fail");
        assert!(matches!(err, MoyasarError::InvalidWebhookSecret));

        let rendered = format!("{err} | {err:?}");
        assert!(!rendered.contains(CANARY_SECRET));
        assert!(!rendered.contains("wrong_secret_xyz"));
    }

    #[test]
    fn tc_9_2_1_missing_secret_token_rejected() {
        let body = br#"{
            "id": "evt_003",
            "type": "payment_paid",
            "live": false
        }"#;
        let err = verify_and_scrub_moyasar_webhook(body, CANARY_SECRET)
            .expect_err("missing secret_token must fail");
        assert!(matches!(err, MoyasarError::InvalidWebhookSecret));
    }

    #[test]
    fn tc_9_2_1_empty_expected_secret_rejected() {
        let body = br#"{
            "id": "evt_004",
            "type": "payment_paid",
            "live": false,
            "secret_token": "wh_secret_123"
        }"#;
        let err = verify_and_scrub_moyasar_webhook(body, "")
            .expect_err("empty expected secret must fail");
        assert!(matches!(err, MoyasarError::InvalidWebhookSecret));
    }

    #[test]
    fn tc_9_2_1_rejects_empty_non_object_and_missing_metadata_payloads() {
        let empty_err =
            verify_and_scrub_moyasar_webhook(b"", CANARY_SECRET).expect_err("empty body");
        assert!(matches!(empty_err, MoyasarError::Api { .. }));

        let array_err =
            verify_and_scrub_moyasar_webhook(b"[1, 2, 3]", CANARY_SECRET).expect_err("json array");
        assert!(matches!(array_err, MoyasarError::Api { .. }));

        let invalid_json_err =
            verify_and_scrub_moyasar_webhook(b"{invalid", CANARY_SECRET).expect_err("bad json");
        assert!(matches!(invalid_json_err, MoyasarError::Api { .. }));

        let missing_id = br#"{
            "id": "",
            "type": "payment_paid",
            "secret_token": "wh_secret_123"
        }"#;
        let id_err =
            verify_and_scrub_moyasar_webhook(missing_id, CANARY_SECRET).expect_err("empty id");
        assert!(matches!(id_err, MoyasarError::Api { .. }));

        let missing_type = br#"{
            "id": "evt_005",
            "secret_token": "wh_secret_123"
        }"#;
        let type_err = verify_and_scrub_moyasar_webhook(missing_type, CANARY_SECRET)
            .expect_err("missing type");
        assert!(matches!(type_err, MoyasarError::Api { .. }));
    }
}
