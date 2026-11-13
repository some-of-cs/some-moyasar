//! Payment Intent data models (`PaymentIntent`, `PaymentIntentStatus`, `IntentId`).

use core::fmt;
use core::str::FromStr;

use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::types::Currency;

/// Fixed, non-configurable Moyasar Payment Intent lifetime in seconds (1 hour).
pub const INTENT_EXPIRY_SECS: i64 = 3600;

/// Strongly-typed, non-empty Payment Intent identifier.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::payment_intents::models::IntentId;
///
/// let id = IntentId::new("pi_test_12345").unwrap();
/// assert_eq!(id.as_str(), "pi_test_12345");
/// assert!(IntentId::new("   ").is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IntentId(String);

impl IntentId {
    /// Validates and constructs an [`IntentId`], rejecting empty or whitespace-only strings.
    pub fn new(id: impl Into<String>) -> Result<Self, MoyasarError> {
        let raw = id.into();
        if raw.trim().is_empty() {
            return Err(MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                message: "payment intent id must not be empty".to_owned(),
                errors: Vec::new(),
                response_code: None,
            });
        }
        Ok(Self(raw))
    }

    /// Returns the underlying string slice of the intent identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IntentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for IntentId {
    type Err = MoyasarError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl TryFrom<&str> for IntentId {
    type Error = MoyasarError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// Lifecycle state of a Moyasar Payment Intent.
///
/// Note: An expired intent still reports [`PaymentIntentStatus::Created`] past its
/// 1-hour `expires_at` timestamp; always check `PaymentIntent::is_usable()` rather
/// than relying on `status` alone. Moreover, [`PaymentIntentStatus::Fulfilled`] only
/// indicates that a fulfillment attempt occurred, **not** that the payment succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaymentIntentStatus {
    /// Intent has been created and may still be usable if `Utc::now() < expires_at`.
    Created,
    /// A fulfillment attempt has been submitted (does not guarantee settlement).
    Fulfilled,
    /// Intent was explicitly canceled via `POST /v1/payment_intents/:id/cancel`.
    Canceled,
    /// Forward-compatible fallback for unrecognized status values.
    #[serde(other)]
    Unknown,
}

/// Server-originated Moyasar Payment Intent locking `amount`, `currency`, `description`,
/// and `callback_url` for tamper-proof browser/mobile checkout.
#[derive(Clone, Deserialize)]
pub struct PaymentIntent {
    /// Unique Moyasar Payment Intent identifier.
    pub id: IntentId,
    /// Current intent lifecycle status.
    pub status: PaymentIntentStatus,
    /// Locked payment amount in minor currency units (e.g., Halalas for SAR).
    pub amount: u64,
    /// Human-readable formatted amount string returned by Moyasar.
    #[serde(default)]
    pub amount_format: Option<String>,
    /// Locked ISO 4217 currency code.
    pub currency: Currency,
    /// Optional locked payment description.
    #[serde(default)]
    pub description: Option<String>,
    /// Locked redirect callback URL for 3DS / completion flows.
    #[serde(default)]
    pub callback_url: Option<String>,
    /// Ephemeral client secret used by the storefront to fulfill the intent.
    #[serde(default)]
    client_secret: Option<SecretString>,
    /// Timestamp after which the intent can no longer be fulfilled or canceled.
    pub expires_at: DateTime<Utc>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp, if any.
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

impl PaymentIntent {
    /// Exposes the `client_secret` string slice when present.
    pub fn client_secret(&self) -> Option<&str> {
        self.client_secret.as_ref().map(ExposeSecret::expose_secret)
    }
}

impl fmt::Debug for PaymentIntent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PaymentIntent")
            .field("id", &self.id)
            .field("status", &self.status)
            .field("amount", &self.amount)
            .field("currency", &self.currency)
            .field("description", &self.description)
            .field("callback_url", &self.callback_url)
            .field("client_secret", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .field("created_at", &self.created_at)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_and_whitespace_intent_ids() {
        assert!(IntentId::new("").is_err());
        assert!(IntentId::new("   \t\n").is_err());
        assert_eq!(IntentId::new("pi_123").expect("valid").as_str(), "pi_123");
    }

    #[test]
    fn deserializes_payment_intent_and_redacts_client_secret_in_debug() {
        let json = r#"{
            "id": "pi_abc123",
            "status": "created",
            "amount": 25000,
            "amount_format": "250.00 SAR",
            "currency": "SAR",
            "description": "Order #42",
            "callback_url": "https://example.com/callback",
            "client_secret": "cs_super_secret_token_value",
            "expires_at": "2026-11-13T12:00:00Z",
            "created_at": "2026-11-13T11:00:00Z"
        }"#;
        let intent: PaymentIntent = serde_json::from_str(json).expect("valid intent json");
        assert_eq!(intent.id.as_str(), "pi_abc123");
        assert_eq!(intent.status, PaymentIntentStatus::Created);
        assert_eq!(intent.amount, 25000);
        assert_eq!(intent.client_secret(), Some("cs_super_secret_token_value"));
        assert_eq!(
            (intent.expires_at - intent.created_at).num_seconds(),
            INTENT_EXPIRY_SECS
        );

        let dbg = format!("{intent:?}");
        assert!(dbg.contains("[REDACTED]"));
        assert!(!dbg.contains("cs_super_secret_token_value"));
    }

    #[test]
    fn maps_unknown_status_to_fallback_variant() {
        let status: PaymentIntentStatus = serde_json::from_str(r#""future_state""#).expect("valid");
        assert_eq!(status, PaymentIntentStatus::Unknown);
    }
}
