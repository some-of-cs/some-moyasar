//! WebhookEventType, WebhookEvent, and WebhookError (US-7.3.0 Unit 1).

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroize;

/// Errors returned by the Moyasar webhook verification and parsing engine.
#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    /// Failed to deserialize the webhook payload JSON.
    #[error("invalid webhook JSON payload: {0}")]
    InvalidJson(#[from] serde_json::Error),

    /// The `secret_token` in the webhook payload did not match the expected secret.
    #[error("webhook secret token mismatch")]
    SecretMismatch,

    /// The webhook `created_at` timestamp fell outside the allowed replay tolerance window.
    #[error("webhook timestamp out of allowed replay window")]
    TimestampOutOfRange,

    /// A webhook event with this ID has already been processed.
    #[error("duplicate webhook event: {0}")]
    DuplicateEvent(Uuid),
}

/// Lookup table mapping Moyasar wire-format event names (including legacy aliases)
/// to [`WebhookEventType`] variants. Canonical names appear before aliases so
/// reverse lookups always yield the canonical wire string.
const KNOWN_WEBHOOK_EVENTS: &[(&str, WebhookEventType)] = &[
    ("payment_paid", WebhookEventType::PaymentPaid),
    ("payment_failed", WebhookEventType::PaymentFailed),
    ("payment_faild", WebhookEventType::PaymentFailed),
    ("payment_refunded", WebhookEventType::PaymentRefunded),
    ("payment_voided", WebhookEventType::PaymentVoided),
    ("payment_authorized", WebhookEventType::PaymentAuthorized),
    ("payment_captured", WebhookEventType::PaymentCaptured),
    ("payment_verified", WebhookEventType::PaymentVerified),
    ("payment_abandoned", WebhookEventType::PaymentAbandoned),
    ("payment_expired", WebhookEventType::PaymentExpired),
    (
        "card_auth_authenticated",
        WebhookEventType::CardAuthAuthenticated,
    ),
    ("card_auth_failed", WebhookEventType::CardAuthFailed),
];

/// Moyasar webhook event types.
///
/// # Examples
///
/// ```
/// use some_moyasar::webhook_engine::event_types::WebhookEventType;
///
/// let paid = WebhookEventType::from("payment_paid");
/// assert_eq!(paid, WebhookEventType::PaymentPaid);
/// assert_eq!(paid.to_string(), "payment_paid");
///
/// let custom = WebhookEventType::from("custom_event");
/// assert_eq!(custom, WebhookEventType::Unknown("custom_event".to_string()));
/// assert_eq!(custom.to_string(), "custom_event");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEventType {
    /// A payment was completed and paid (`"payment_paid"`).
    PaymentPaid,

    /// A payment attempt failed (`"payment_failed"`, or Moyasar's legacy `"payment_faild"` typo).
    #[serde(rename = "payment_failed", alias = "payment_faild")]
    PaymentFailed,

    /// A payment was refunded (`"payment_refunded"`).
    PaymentRefunded,

    /// A payment was voided (`"payment_voided"`).
    PaymentVoided,

    /// A payment was authorized (`"payment_authorized"`).
    PaymentAuthorized,

    /// An authorized payment was captured (`"payment_captured"`).
    PaymentCaptured,

    /// A payment was verified (`"payment_verified"`).
    PaymentVerified,

    /// A payment was abandoned by the customer (`"payment_abandoned"`).
    PaymentAbandoned,

    /// A payment expired before completion (`"payment_expired"`).
    PaymentExpired,

    /// Card 3DS authentication succeeded (`"card_auth_authenticated"`).
    CardAuthAuthenticated,

    /// Card 3DS authentication failed (`"card_auth_failed"`).
    CardAuthFailed,

    /// Any unrecognized event type string preserved verbatim.
    #[serde(untagged)]
    Unknown(String),
}

impl WebhookEventType {
    /// Returns the canonical wire-format string representation of this event type.
    #[must_use]
    pub fn as_str(&self) -> &str {
        if let Self::Unknown(s) = self {
            return s.as_str();
        }
        KNOWN_WEBHOOK_EVENTS
            .iter()
            .find(|(_, variant)| variant == self)
            .map_or("", |(name, _)| *name)
    }
}

impl From<&str> for WebhookEventType {
    fn from(s: &str) -> Self {
        KNOWN_WEBHOOK_EVENTS
            .iter()
            .find(|(name, _)| *name == s)
            .map(|(_, variant)| variant.clone())
            .unwrap_or_else(|| Self::Unknown(s.to_owned()))
    }
}

impl fmt::Display for WebhookEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A parsed Moyasar webhook event envelope.
///
/// The [`secret_token`](Self::secret_token) field is redacted as `"[REDACTED]"` in
/// [`fmt::Debug`] output and zeroized in memory on [`Drop`] to prevent accidental
/// secret leakage.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookEvent {
    /// Unique identifier of the webhook event.
    pub id: Uuid,

    /// Event type classification.
    #[serde(rename = "type")]
    pub event_type: WebhookEventType,

    /// UTC timestamp when the event was created by Moyasar.
    pub created_at: DateTime<Utc>,

    /// Shared secret token sent by Moyasar for webhook authentication.
    pub secret_token: String,

    /// Optional merchant account name associated with the event.
    #[serde(default)]
    pub account_name: Option<String>,

    /// Whether the event originated in live mode (`true`) or test mode (`false`).
    pub live: bool,

    /// Raw JSON payload for the event's `data` object.
    pub data: serde_json::Value,
}

impl Drop for WebhookEvent {
    fn drop(&mut self) {
        self.secret_token.zeroize();
    }
}

impl fmt::Debug for WebhookEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebhookEvent")
            .field("id", &self.id)
            .field("event_type", &self.event_type)
            .field("created_at", &self.created_at)
            .field("secret_token", &"[REDACTED]")
            .field("account_name", &self.account_name)
            .field("live", &self.live)
            .field("data", &self.data)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_event_types_round_trip() {
        let cases = [
            ("payment_paid", WebhookEventType::PaymentPaid),
            ("payment_failed", WebhookEventType::PaymentFailed),
            ("payment_refunded", WebhookEventType::PaymentRefunded),
            ("payment_voided", WebhookEventType::PaymentVoided),
            ("payment_authorized", WebhookEventType::PaymentAuthorized),
            ("payment_captured", WebhookEventType::PaymentCaptured),
            ("payment_verified", WebhookEventType::PaymentVerified),
            ("payment_abandoned", WebhookEventType::PaymentAbandoned),
            ("payment_expired", WebhookEventType::PaymentExpired),
            (
                "card_auth_authenticated",
                WebhookEventType::CardAuthAuthenticated,
            ),
            ("card_auth_failed", WebhookEventType::CardAuthFailed),
        ];

        for (wire, expected) in cases {
            let json = format!("\"{wire}\"");
            let parsed: WebhookEventType = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, expected);
            assert_eq!(WebhookEventType::from(wire), expected);
            assert_eq!(parsed.to_string(), wire);
            assert_eq!(serde_json::to_string(&parsed).unwrap(), json);
        }
    }

    #[test]
    fn deserializes_payment_faild_alias() {
        let parsed: WebhookEventType = serde_json::from_str("\"payment_faild\"").unwrap();
        assert_eq!(parsed, WebhookEventType::PaymentFailed);
        assert_eq!(
            WebhookEventType::from("payment_faild"),
            WebhookEventType::PaymentFailed
        );
        assert_eq!(parsed.to_string(), "payment_failed");
        assert_eq!(
            serde_json::to_string(&parsed).unwrap(),
            "\"payment_failed\""
        );
    }

    #[test]
    fn deserializes_unknown_variant_verbatim() {
        let parsed: WebhookEventType = serde_json::from_str("\"subscription_renewed\"").unwrap();
        assert_eq!(
            parsed,
            WebhookEventType::Unknown("subscription_renewed".to_owned())
        );
        assert_eq!(
            WebhookEventType::from("subscription_renewed"),
            WebhookEventType::Unknown("subscription_renewed".to_owned())
        );
        assert_eq!(parsed.to_string(), "subscription_renewed");
        assert_eq!(
            serde_json::to_string(&parsed).unwrap(),
            "\"subscription_renewed\""
        );
    }

    #[test]
    fn debug_redacts_secret_token() {
        let event = WebhookEvent {
            id: Uuid::nil(),
            event_type: WebhookEventType::PaymentPaid,
            created_at: DateTime::UNIX_EPOCH,
            secret_token: "whsec_super_secret_value_12345".to_owned(),
            account_name: Some("moyasar_merchant".to_owned()),
            live: false,
            data: serde_json::json!({"id": "pay_123", "amount": 1000}),
        };

        let debug_output = format!("{event:?}");
        assert!(!debug_output.contains("whsec_super_secret_value_12345"));
        assert!(debug_output.contains("[REDACTED]"));
        assert!(debug_output.contains("moyasar_merchant"));
    }

    #[test]
    fn deserializes_full_webhook_event_payload() {
        let payload = r#"{
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "type": "payment_faild",
            "created_at": "2026-11-14T03:00:00Z",
            "secret_token": "tok_secret_abc",
            "account_name": null,
            "live": true,
            "data": { "id": "pay_999", "status": "failed" }
        }"#;

        let event: WebhookEvent = serde_json::from_str(payload).unwrap();
        assert_eq!(
            event.id,
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap()
        );
        assert_eq!(event.event_type, WebhookEventType::PaymentFailed);
        assert_eq!(event.secret_token, "tok_secret_abc");
        assert_eq!(event.account_name, None);
        assert!(event.live);
        assert_eq!(event.data["id"], "pay_999");
    }

    #[test]
    fn webhook_error_variants_format_cleanly() {
        let json_err = serde_json::from_str::<WebhookEvent>("not-json").unwrap_err();
        let err = WebhookError::from(json_err);
        assert!(matches!(err, WebhookError::InvalidJson(_)));
        assert!(err.to_string().contains("invalid webhook JSON payload"));

        assert_eq!(
            WebhookError::SecretMismatch.to_string(),
            "webhook secret token mismatch"
        );
        assert_eq!(
            WebhookError::TimestampOutOfRange.to_string(),
            "webhook timestamp out of allowed replay window"
        );
        let dup_id = Uuid::nil();
        assert_eq!(
            WebhookError::DuplicateEvent(dup_id).to_string(),
            format!("duplicate webhook event: {dup_id}")
        );
    }
}
