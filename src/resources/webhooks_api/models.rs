//! WebhookEndpoint, WebhookAttempt, and WebhookSecret models (US-7.2.0 Unit 1).

use core::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::webhook_engine::event_types::WebhookEventType;

/// Redacted wrapper for a Moyasar webhook HMAC shared secret.
///
/// The underlying secret material is zeroized on drop, and [`fmt::Debug`]
/// emits `"[REDACTED]"` to prevent accidental leakage into logs or traces.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(transparent)]
pub struct WebhookSecret(String);

impl WebhookSecret {
    /// Creates a new [`WebhookSecret`] from any string-like value.
    #[must_use]
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    /// Exposes the raw shared secret string for cryptographic verification or API payloads.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for WebhookSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

/// Moyasar webhook endpoint resource returned by `/v1/webhooks`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookEndpoint {
    /// Unique identifier of the webhook endpoint.
    pub id: Uuid,
    /// HTTP method used for webhook delivery (e.g., `"POST"`).
    pub http_method: String,
    /// Destination URL receiving webhook event payloads.
    pub url: String,
    /// Optional HMAC shared secret used to sign webhook payloads.
    #[serde(default)]
    pub shared_secret: Option<WebhookSecret>,
    /// Optional list of subscribed webhook event types.
    #[serde(default)]
    pub events: Option<Vec<WebhookEventType>>,
    /// Timestamp when the webhook endpoint was created.
    pub created_at: DateTime<Utc>,
}

/// Moyasar webhook delivery attempt record returned by `/v1/webhooks/attempts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookAttempt {
    /// Unique identifier of the delivery attempt.
    pub id: Uuid,
    /// Identifier of the target webhook endpoint, if available.
    #[serde(default)]
    pub webhook_id: Option<Uuid>,
    /// Identifier of the webhook event that triggered this attempt.
    pub event_id: Uuid,
    /// Event type associated with this delivery attempt, if available.
    #[serde(default)]
    pub event_type: Option<WebhookEventType>,
    /// HTTP status code returned by the destination endpoint.
    #[serde(default)]
    pub response_code: Option<u16>,
    /// HTTP response headers returned by the destination endpoint.
    #[serde(default)]
    pub response_headers: Option<serde_json::Value>,
    /// Raw HTTP response body returned by the destination endpoint.
    #[serde(default)]
    pub response_body: Option<String>,
    /// Whether the webhook delivery attempt succeeded.
    #[serde(default)]
    pub succeeded: Option<bool>,
    /// Diagnostic or error message associated with the delivery attempt.
    #[serde(default)]
    pub message: Option<String>,
    /// Timestamp when the delivery attempt was created.
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tc_7_2_0_law_webhook_secret_redaction_and_serde_roundtrip() {
        let raw = "whsec_test_9876543210abcdef";
        let secret = WebhookSecret::new(raw);

        assert_eq!(format!("{secret:?}"), "[REDACTED]");
        assert_eq!(secret.expose(), raw);

        let serialized = serde_json::to_string(&secret).expect("serialize WebhookSecret");
        assert_eq!(serialized, format!("\"{raw}\""));

        let deserialized: WebhookSecret =
            serde_json::from_str(&serialized).expect("deserialize WebhookSecret");
        assert_eq!(deserialized, secret);
        assert_eq!(format!("{deserialized:?}"), "[REDACTED]");
    }

    #[test]
    fn webhook_endpoint_and_attempt_roundtrip_without_leaking_secret() {
        let endpoint_json = json!({
            "id": "11111111-1111-4111-8111-111111111111",
            "http_method": "POST",
            "url": "https://merchant.example/webhooks/moyasar",
            "shared_secret": "whsec_endpoint_secret",
            "events": null,
            "created_at": "2026-11-14T00:00:00Z"
        });

        let endpoint: WebhookEndpoint =
            serde_json::from_value(endpoint_json).expect("deserialize WebhookEndpoint");
        assert_eq!(endpoint.http_method, "POST");
        assert_eq!(
            endpoint.shared_secret.as_ref().map(WebhookSecret::expose),
            Some("whsec_endpoint_secret")
        );
        let endpoint_dbg = format!("{endpoint:?}");
        assert!(!endpoint_dbg.contains("whsec_endpoint_secret"));
        assert!(endpoint_dbg.contains("[REDACTED]"));

        let attempt_json = json!({
            "id": "22222222-2222-4222-8222-222222222222",
            "webhook_id": "11111111-1111-4111-8111-111111111111",
            "event_id": "33333333-3333-4333-8333-333333333333",
            "event_type": null,
            "response_code": 200,
            "response_headers": { "content-type": "application/json" },
            "response_body": "{\"ok\":true}",
            "succeeded": true,
            "message": "Delivered",
            "created_at": "2026-11-14T00:01:00Z"
        });

        let attempt: WebhookAttempt =
            serde_json::from_value(attempt_json).expect("deserialize WebhookAttempt");
        assert_eq!(attempt.response_code, Some(200));
        assert_eq!(attempt.succeeded, Some(true));
        assert_eq!(attempt.message.as_deref(), Some("Delivered"));
    }
}
