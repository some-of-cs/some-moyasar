//! Live webhook event lookup table (`payment_faild` tolerant) and verification handshake helpers
//! (US-9.2.0 Unit 2).
//!
//! Provides:
//! - [`NormalizedMoyasarEvent`] and [`normalize_moyasar_event`] backed by the static
//!   [`KNOWN_MOYASAR_EVENTS`] table covering all 10 live Moyasar `/v1/webhooks/available_events`
//!   slugs, the historical `"payment_faild"` typo, and `"webhook_verification"`.
//! - [`HandshakeChallenge`] (with [`zeroize::ZeroizeOnDrop`] and redacted [`fmt::Debug`]),
//!   [`validate_verification_code`], and [`extract_verification_challenge`] for offline-validated
//!   6-digit verification handshake parsing (protecting Moyasar's 5-attempt limit).
//! - [`verify_webhook`] and [`resend_verification`] async HTTP helpers against Moyasar's
//!   `/webhooks/{id}/verify` and `/webhooks/{id}/resend_verification` endpoints.

use std::fmt;

use crate::kernel::error::MoyasarError;

/// Canonical Moyasar webhook event variants normalized from live wire slugs.
///
/// Covers all 10 live `/v1/webhooks/available_events` event types (normalizing the legacy
/// `"payment_faild"` typo to [`NormalizedMoyasarEvent::PaymentFailed`]) plus the
/// `"webhook_verification"` handshake event.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::webhook_handshake::{
///     NormalizedMoyasarEvent, normalize_moyasar_event,
/// };
///
/// assert_eq!(
///     normalize_moyasar_event("payment_faild"),
///     Some(NormalizedMoyasarEvent::PaymentFailed)
/// );
/// assert_eq!(
///     NormalizedMoyasarEvent::PaymentFailed.as_str(),
///     "payment_failed"
/// );
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NormalizedMoyasarEvent {
    /// Payment completed and paid (`"payment_paid"`).
    PaymentPaid,
    /// Payment attempt failed (`"payment_failed"` or legacy `"payment_faild"` typo).
    PaymentFailed,
    /// Payment authorized (`"payment_authorized"`).
    PaymentAuthorized,
    /// Authorized payment captured (`"payment_captured"`).
    PaymentCaptured,
    /// Payment refunded (`"payment_refunded"`).
    PaymentRefunded,
    /// Payment voided (`"payment_voided"`).
    PaymentVoided,
    /// Payment abandoned by the customer (`"payment_abandoned"`).
    PaymentAbandoned,
    /// Payment verified (`"payment_verified"`).
    PaymentVerified,
    /// Merchant balance transferred (`"balance_transferred"`).
    BalanceTransferred,
    /// Payout paid (`"payout_paid"`).
    PayoutPaid,
    /// Webhook endpoint verification handshake challenge (`"webhook_verification"`).
    WebhookVerification,
}

/// Static lookup table mapping all 10 live Moyasar `/v1/webhooks/available_events` slugs,
/// the `"payment_faild"` typo alias, and `"webhook_verification"` to [`NormalizedMoyasarEvent`].
///
/// Canonical slugs always appear before aliases so reverse lookups via
/// [`NormalizedMoyasarEvent::as_str`] return the canonical Moyasar slug.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::webhook_handshake::KNOWN_MOYASAR_EVENTS;
///
/// assert_eq!(KNOWN_MOYASAR_EVENTS.len(), 12);
/// ```
pub const KNOWN_MOYASAR_EVENTS: &[(&str, NormalizedMoyasarEvent)] = &[
    ("payment_paid", NormalizedMoyasarEvent::PaymentPaid),
    ("payment_failed", NormalizedMoyasarEvent::PaymentFailed),
    ("payment_faild", NormalizedMoyasarEvent::PaymentFailed),
    (
        "payment_authorized",
        NormalizedMoyasarEvent::PaymentAuthorized,
    ),
    ("payment_captured", NormalizedMoyasarEvent::PaymentCaptured),
    ("payment_refunded", NormalizedMoyasarEvent::PaymentRefunded),
    ("payment_voided", NormalizedMoyasarEvent::PaymentVoided),
    (
        "payment_abandoned",
        NormalizedMoyasarEvent::PaymentAbandoned,
    ),
    ("payment_verified", NormalizedMoyasarEvent::PaymentVerified),
    (
        "balance_transferred",
        NormalizedMoyasarEvent::BalanceTransferred,
    ),
    ("payout_paid", NormalizedMoyasarEvent::PayoutPaid),
    (
        "webhook_verification",
        NormalizedMoyasarEvent::WebhookVerification,
    ),
];

impl NormalizedMoyasarEvent {
    /// Returns the canonical lowercase snake_case wire slug for this event variant.
    ///
    /// # Examples
    ///
    /// ```
    /// use some_moyasar::alignment::webhook_handshake::NormalizedMoyasarEvent;
    ///
    /// assert_eq!(
    ///     NormalizedMoyasarEvent::WebhookVerification.as_str(),
    ///     "webhook_verification"
    /// );
    /// ```
    #[must_use]
    pub fn as_str(self) -> &'static str {
        KNOWN_MOYASAR_EVENTS
            .iter()
            .find(|(_, variant)| *variant == self)
            .map_or("", |(slug, _)| *slug)
    }
}

impl fmt::Display for NormalizedMoyasarEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Normalizes a raw Moyasar webhook event slug using a case-insensitive, trimmed lookup
/// against [`KNOWN_MOYASAR_EVENTS`].
///
/// Returns `None` for unrecognized event strings.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::webhook_handshake::{
///     NormalizedMoyasarEvent, normalize_moyasar_event,
/// };
///
/// assert_eq!(
///     normalize_moyasar_event("  PAYMENT_FAILD  "),
///     Some(NormalizedMoyasarEvent::PaymentFailed)
/// );
/// assert_eq!(
///     normalize_moyasar_event("webhook_verification"),
///     Some(NormalizedMoyasarEvent::WebhookVerification)
/// );
/// assert_eq!(normalize_moyasar_event("unknown_event"), None);
/// ```
#[must_use]
pub fn normalize_moyasar_event(event_type: &str) -> Option<NormalizedMoyasarEvent> {
    let trimmed = event_type.trim();
    KNOWN_MOYASAR_EVENTS
        .iter()
        .find(|(slug, _)| slug.eq_ignore_ascii_case(trimmed))
        .map(|(_, variant)| *variant)
}

/// Parsed webhook verification handshake challenge containing the target webhook ID and its
/// 6-digit verification code.
///
/// Memory is wiped on drop via [`zeroize::ZeroizeOnDrop`], and [`fmt::Debug`] redacts
/// [`verification_code`](Self::verification_code) as `"[REDACTED]"` to prevent log leakage.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::webhook_handshake::HandshakeChallenge;
///
/// let challenge = HandshakeChallenge {
///     webhook_id: "wh_123".to_owned(),
///     verification_code: "654321".to_owned(),
/// };
/// let dbg = format!("{challenge:?}");
/// assert!(dbg.contains("wh_123"));
/// assert!(dbg.contains("[REDACTED]"));
/// assert!(!dbg.contains("654321"));
/// ```
#[derive(Clone, PartialEq, Eq, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct HandshakeChallenge {
    /// Target Moyasar webhook identifier.
    pub webhook_id: String,
    /// Strictly 6-digit ASCII numeric verification code sent by Moyasar.
    pub verification_code: String,
}

impl fmt::Debug for HandshakeChallenge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HandshakeChallenge")
            .field("webhook_id", &self.webhook_id)
            .field("verification_code", &"[REDACTED]")
            .finish()
    }
}

/// Validates that `code` consists of strictly 6 ASCII digits before making any API call.
///
/// Enforcing this invariant locally prevents callers from consuming Moyasar's 5-attempt
/// verification lockout limit with malformed codes.
///
/// # Errors
///
/// Returns [`MoyasarError::Api`] with `InvalidRequestError` if `code` is not exactly 6 ASCII
/// digits (`0..=9`).
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::webhook_handshake::validate_verification_code;
///
/// assert!(validate_verification_code("123456").is_ok());
/// assert!(validate_verification_code("12345").is_err());
/// assert!(validate_verification_code("abcdef").is_err());
/// ```
pub fn validate_verification_code(code: &str) -> Result<(), MoyasarError> {
    if code.len() == 6 && code.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(());
    }
    Err(MoyasarError::invalid_request(
        "verification_code must be strictly 6 ASCII digits",
    ))
}

fn validate_webhook_id(webhook_id: &str) -> Result<&str, MoyasarError> {
    let trimmed = webhook_id.trim();
    if trimmed.is_empty() {
        return Err(MoyasarError::invalid_request(
            "webhook_id must not be empty",
        ));
    }
    Ok(trimmed)
}

fn first_non_empty_trimmed_str<'a>(candidates: &[Option<&'a serde_json::Value>]) -> Option<&'a str> {
    candidates
        .iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::trim))
        .find(|s| !s.is_empty())
}

fn first_str<'a>(candidates: &[Option<&'a serde_json::Value>]) -> Option<&'a str> {
    candidates
        .iter()
        .flatten()
        .find_map(serde_json::Value::as_str)
}

/// Extracts a validated [`HandshakeChallenge`] from a Moyasar webhook verification JSON payload.
///
/// Looks up `webhook_id` from `data.webhook_id`, `data.id`, or top-level `webhook_id`, and
/// `verification_code` from `data.verification_code` or top-level `verification_code`, then
/// validates the code with [`validate_verification_code`].
///
/// # Errors
///
/// Returns [`MoyasarError::Api`] with `InvalidRequestError` if `webhook_id` is missing/empty,
/// `verification_code` is missing, or `verification_code` is not strictly 6 ASCII digits.
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// use some_moyasar::alignment::webhook_handshake::extract_verification_challenge;
///
/// let payload = json!({
///     "type": "webhook_verification",
///     "data": {
///         "webhook_id": "wh_live_001",
///         "verification_code": "123456"
///     }
/// });
/// let challenge = extract_verification_challenge(&payload).unwrap();
/// assert_eq!(challenge.webhook_id, "wh_live_001");
/// assert_eq!(challenge.verification_code, "123456");
/// ```
pub fn extract_verification_challenge(
    payload: &serde_json::Value,
) -> Result<HandshakeChallenge, MoyasarError> {
    let data = payload.get("data");
    let webhook_id = first_non_empty_trimmed_str(&[
        data.and_then(|d| d.get("webhook_id")),
        data.and_then(|d| d.get("id")),
        payload.get("webhook_id"),
    ])
    .ok_or_else(|| MoyasarError::invalid_request("missing webhook_id in verification payload"))?;

    let verification_code = first_str(&[
        data.and_then(|d| d.get("verification_code")),
        payload.get("verification_code"),
    ])
    .ok_or_else(|| {
        MoyasarError::invalid_request("missing verification_code in verification payload")
    })?;

    validate_verification_code(verification_code)?;

    Ok(HandshakeChallenge {
        webhook_id: webhook_id.to_owned(),
        verification_code: verification_code.to_owned(),
    })
}

async fn map_empty_response(resp: reqwest::Response) -> Result<(), MoyasarError> {
    let status = resp.status();
    if status.is_success() {
        return Ok(());
    }
    let body = resp.text().await.unwrap_or_default();
    Err(MoyasarError::from_api_response(status.as_u16(), &body))
}

/// Submits a 6-digit verification code to `POST {base_url}/webhooks/{webhook_id}/verify`.
///
/// Validates `webhook_id` (non-empty) and `verification_code` (strictly 6 ASCII digits) locally
/// before issuing any HTTP request so invalid codes never burn Moyasar's 5-attempt limit.
///
/// # Errors
///
/// - Returns [`MoyasarError::Api`] (`InvalidRequestError`) locally without making an HTTP call
///   when `webhook_id` is empty or `verification_code` fails [`validate_verification_code`].
/// - Returns [`MoyasarError::Network`] or [`MoyasarError::Timeout`] on transport failures.
/// - Maps non-2xx API responses via [`MoyasarError::from_api_response`].
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::webhook_handshake::verify_webhook;
///
/// let rt = tokio::runtime::Builder::new_current_thread()
///     .build()
///     .unwrap();
/// let http = reqwest::Client::new();
/// // Local validation rejects malformed codes with zero HTTP calls.
/// let err = rt.block_on(verify_webhook(
///     &http,
///     "https://api.moyasar.com/v1",
///     "sk_test_example",
///     "wh_123",
///     "bad",
/// ));
/// assert!(err.is_err());
/// ```
pub async fn verify_webhook(
    http: &reqwest::Client,
    base_url: &str,
    secret_key: &str,
    webhook_id: &str,
    verification_code: &str,
) -> Result<(), MoyasarError> {
    let webhook_id = validate_webhook_id(webhook_id)?;
    validate_verification_code(verification_code)?;

    let base = base_url.trim_end_matches('/');
    let url = format!("{base}/webhooks/{webhook_id}/verify");
    let resp = http
        .post(url)
        .basic_auth(secret_key, Some(""))
        .json(&serde_json::json!({ "verification_code": verification_code }))
        .send()
        .await?;

    map_empty_response(resp).await
}

/// Requests Moyasar to resend the 6-digit webhook verification challenge via
/// `POST {base_url}/webhooks/{webhook_id}/resend_verification`.
///
/// # Errors
///
/// - Returns [`MoyasarError::Api`] (`InvalidRequestError`) locally when `webhook_id` is empty.
/// - Returns [`MoyasarError::Network`] or [`MoyasarError::Timeout`] on transport failures.
/// - Maps non-2xx API responses via [`MoyasarError::from_api_response`].
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::webhook_handshake::resend_verification;
///
/// let rt = tokio::runtime::Builder::new_current_thread()
///     .build()
///     .unwrap();
/// let http = reqwest::Client::new();
/// // Local validation rejects empty webhook IDs with zero HTTP calls.
/// let err = rt.block_on(resend_verification(
///     &http,
///     "https://api.moyasar.com/v1",
///     "sk_test_example",
///     "   ",
/// ));
/// assert!(err.is_err());
/// ```
pub async fn resend_verification(
    http: &reqwest::Client,
    base_url: &str,
    secret_key: &str,
    webhook_id: &str,
) -> Result<(), MoyasarError> {
    let webhook_id = validate_webhook_id(webhook_id)?;
    let base = base_url.trim_end_matches('/');
    let url = format!("{base}/webhooks/{webhook_id}/resend_verification");
    let resp = http
        .post(url)
        .basic_auth(secret_key, Some(""))
        .send()
        .await?;

    map_empty_response(resp).await
}

#[cfg(test)]
mod tests {
    use base64::Engine as _;
    use serde_json::json;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::kernel::error::ApiErrorType;

    fn expected_basic_header(secret_key: &str) -> String {
        let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{secret_key}:"));
        format!("Basic {encoded}")
    }

    #[test]
    fn tc_9_2_2_normalizes_all_ten_live_events_typo_and_verification() {
        let cases = [
            ("payment_paid", NormalizedMoyasarEvent::PaymentPaid),
            ("payment_failed", NormalizedMoyasarEvent::PaymentFailed),
            ("payment_faild", NormalizedMoyasarEvent::PaymentFailed),
            (
                "payment_authorized",
                NormalizedMoyasarEvent::PaymentAuthorized,
            ),
            ("payment_captured", NormalizedMoyasarEvent::PaymentCaptured),
            ("payment_refunded", NormalizedMoyasarEvent::PaymentRefunded),
            ("payment_voided", NormalizedMoyasarEvent::PaymentVoided),
            (
                "payment_abandoned",
                NormalizedMoyasarEvent::PaymentAbandoned,
            ),
            ("payment_verified", NormalizedMoyasarEvent::PaymentVerified),
            (
                "balance_transferred",
                NormalizedMoyasarEvent::BalanceTransferred,
            ),
            ("payout_paid", NormalizedMoyasarEvent::PayoutPaid),
            (
                "webhook_verification",
                NormalizedMoyasarEvent::WebhookVerification,
            ),
        ];

        assert_eq!(KNOWN_MOYASAR_EVENTS.len(), 12);
        for (slug, expected) in cases {
            assert_eq!(normalize_moyasar_event(slug), Some(expected));
            let noisy = format!("  {}  ", slug.to_ascii_uppercase());
            assert_eq!(normalize_moyasar_event(&noisy), Some(expected));
        }

        assert_eq!(
            NormalizedMoyasarEvent::PaymentFailed.as_str(),
            "payment_failed"
        );
        assert_eq!(
            NormalizedMoyasarEvent::PaymentFailed.to_string(),
            "payment_failed"
        );
        assert_eq!(normalize_moyasar_event("unknown_event"), None);
        assert_eq!(normalize_moyasar_event("   "), None);
    }

    #[test]
    fn tc_9_2_2_validates_six_ascii_digit_codes() {
        assert!(validate_verification_code("123456").is_ok());
        assert!(validate_verification_code("000000").is_ok());

        for bad in [
            "12345",
            "1234567",
            "abcdef",
            "12345a",
            "",
            " 12345",
            "١٢٣٤٥٦",
        ] {
            let err = validate_verification_code(bad).unwrap_err();
            assert!(matches!(
                err,
                MoyasarError::Api {
                    error_type: ApiErrorType::InvalidRequestError,
                    ..
                }
            ));
            assert!(!err.to_string().contains(bad) || bad.is_empty());
        }
    }

    #[test]
    fn tc_9_2_2_extracts_verification_challenge_from_nested_and_top_level_shapes() {
        let nested_webhook_id = json!({
            "type": "webhook_verification",
            "data": {
                "webhook_id": "wh_nested_1",
                "verification_code": "123456"
            }
        });
        assert_eq!(
            extract_verification_challenge(&nested_webhook_id).unwrap(),
            HandshakeChallenge {
                webhook_id: "wh_nested_1".to_owned(),
                verification_code: "123456".to_owned(),
            }
        );

        let nested_id = json!({
            "type": "webhook_verification",
            "data": {
                "id": "wh_nested_id_2",
                "verification_code": "654321"
            }
        });
        assert_eq!(
            extract_verification_challenge(&nested_id).unwrap(),
            HandshakeChallenge {
                webhook_id: "wh_nested_id_2".to_owned(),
                verification_code: "654321".to_owned(),
            }
        );

        let top_level = json!({
            "webhook_id": "wh_top_3",
            "verification_code": "000999"
        });
        assert_eq!(
            extract_verification_challenge(&top_level).unwrap(),
            HandshakeChallenge {
                webhook_id: "wh_top_3".to_owned(),
                verification_code: "000999".to_owned(),
            }
        );

        let missing_id = json!({ "data": { "verification_code": "123456" } });
        assert!(extract_verification_challenge(&missing_id).is_err());

        let missing_code = json!({ "data": { "webhook_id": "wh_1" } });
        assert!(extract_verification_challenge(&missing_code).is_err());

        let bad_code = json!({
            "data": { "webhook_id": "wh_1", "verification_code": "12345" }
        });
        assert!(extract_verification_challenge(&bad_code).is_err());
    }

    #[test]
    fn tc_9_2_2_debug_redacts_verification_code_canary() {
        let challenge = HandshakeChallenge {
            webhook_id: "wh_canary_42".to_owned(),
            verification_code: "987654".to_owned(),
        };
        let rendered = format!("{challenge:?}");
        assert!(rendered.contains("wh_canary_42"));
        assert!(rendered.contains("[REDACTED]"));
        assert!(!rendered.contains("987654"));
    }

    #[tokio::test]
    async fn tc_9_2_2_verify_webhook_sends_basic_auth_and_json_body() {
        let server = MockServer::start().await;
        let secret = "sk_test_handshake_verify";

        Mock::given(method("POST"))
            .and(path("/v1/webhooks/wh_verify_1/verify"))
            .and(header("authorization", expected_basic_header(secret)))
            .and(body_json(json!({ "verification_code": "123456" })))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let base_url = format!("{}/v1", server.uri());
        verify_webhook(&http, &base_url, secret, "wh_verify_1", "123456")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn tc_9_2_2_verify_webhook_rejects_bad_code_with_zero_http_calls() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let base_url = format!("{}/v1", server.uri());

        assert!(
            verify_webhook(&http, &base_url, "sk_test_key", "wh_1", "12345")
                .await
                .is_err()
        );
        assert!(
            verify_webhook(&http, &base_url, "sk_test_key", "wh_1", "abcdef")
                .await
                .is_err()
        );
        assert!(
            verify_webhook(&http, &base_url, "sk_test_key", "   ", "123456")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn tc_9_2_2_verify_webhook_maps_non_2xx_api_errors() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/webhooks/wh_bad_code/verify"))
            .respond_with(ResponseTemplate::new(422).set_body_json(json!({
                "type": "invalid_request_error",
                "message": "Verification code is invalid"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let base_url = format!("{}/v1", server.uri());
        let err = verify_webhook(&http, &base_url, "sk_test_key", "wh_bad_code", "111111")
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn tc_9_2_2_resend_verification_sends_post_and_maps_errors() {
        let server = MockServer::start().await;
        let secret = "sk_test_handshake_resend";

        Mock::given(method("POST"))
            .and(path("/v1/webhooks/wh_resend_1/resend_verification"))
            .and(header("authorization", expected_basic_header(secret)))
            .respond_with(ResponseTemplate::new(202))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/webhooks/wh_missing/resend_verification"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "type": "record_not_found",
                "message": "Webhook not found"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let base_url = format!("{}/v1/", server.uri());
        resend_verification(&http, &base_url, secret, "wh_resend_1")
            .await
            .unwrap();

        let err = resend_verification(&http, &base_url, secret, "wh_missing")
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::RecordNotFound,
                ..
            }
        ));

        assert!(
            resend_verification(&http, &base_url, secret, "")
                .await
                .is_err()
        );
    }
}
