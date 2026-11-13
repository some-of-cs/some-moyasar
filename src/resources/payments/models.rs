//! Payment response models and lifecycle status enumeration (US-2.1.0 Unit 1).

use core::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::kernel::metadata::Metadata;
use crate::kernel::types::Currency;

/// Lifecycle state of a Moyasar payment.
///
/// Unknown wire statuses deserialize into [`PaymentStatus::Unknown`] to preserve
/// forward compatibility with future Moyasar API states.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PaymentStatus {
    /// Payment has been initiated and awaits payer action or 3DS completion.
    Initiated,
    /// Payment has been paid and settled.
    Paid,
    /// Payment has been authorized (funds held) awaiting capture or void.
    Authorized,
    /// Previously authorized payment has been captured.
    Captured,
    /// Previously authorized payment has been voided.
    Voided,
    /// Payment has been refunded (partially or in full).
    Refunded,
    /// Payment attempt failed.
    Failed,
    /// Card or token verification succeeded without charge.
    Verified,
    /// Payment expired before completion.
    Expired,
    /// Forward-compatible fallback carrying the unrecognized wire status string.
    Unknown(String),
}

const KNOWN_STATUSES: &[(&str, PaymentStatus)] = &[
    ("initiated", PaymentStatus::Initiated),
    ("paid", PaymentStatus::Paid),
    ("authorized", PaymentStatus::Authorized),
    ("captured", PaymentStatus::Captured),
    ("voided", PaymentStatus::Voided),
    ("refunded", PaymentStatus::Refunded),
    ("failed", PaymentStatus::Failed),
    ("verified", PaymentStatus::Verified),
    ("expired", PaymentStatus::Expired),
];

impl PaymentStatus {
    /// Parses a wire-format status string into [`PaymentStatus`].
    pub fn from_wire(raw: &str) -> Self {
        KNOWN_STATUSES
            .iter()
            .find(|(wire, _)| *wire == raw)
            .map(|(_, status)| status.clone())
            .unwrap_or_else(|| Self::Unknown(raw.to_owned()))
    }

    /// Returns the wire-format lowercase status string.
    pub fn as_str(&self) -> &str {
        if let Self::Unknown(raw) = self {
            return raw.as_str();
        }
        KNOWN_STATUSES
            .iter()
            .find(|(_, status)| status == self)
            .map_or("unknown", |(wire, _)| *wire)
    }
}

impl fmt::Display for PaymentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for PaymentStatus {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PaymentStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::from_wire(&raw))
    }
}

/// Moyasar Payment resource returned by `POST /v1/payments`, `GET /v1/payments/:id`,
/// and `GET /v1/payments`.
///
/// Monetary fields expose both integer minor units (`amount`, `fee`, `refunded`,
/// `captured`) and Moyasar's pre-formatted display strings (`*_format`).
/// `Debug` redacts `metadata` values and payer `ip` to prevent PII log leakage.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct Payment {
    /// Unique Moyasar payment identifier (e.g., `pay_...` or UUID).
    pub id: String,
    /// Current lifecycle status of the payment.
    pub status: PaymentStatus,
    /// Payment amount in the currency's smallest minor unit (e.g., Halalas for SAR).
    pub amount: u64,
    /// Pre-formatted human-readable payment amount string (e.g., `"150.00"`).
    pub amount_format: String,
    /// Processing fee in minor units.
    pub fee: u64,
    /// Pre-formatted processing fee string.
    pub fee_format: String,
    /// Total refunded amount in minor units.
    pub refunded: u64,
    /// Pre-formatted refunded amount string.
    pub refunded_format: String,
    /// Total captured amount in minor units.
    pub captured: u64,
    /// Pre-formatted captured amount string.
    pub captured_format: String,
    /// ISO 4217 currency code.
    pub currency: Currency,
    /// Optional merchant-supplied description.
    #[serde(default)]
    pub description: Option<String>,
    /// Polymorphic payment source response payload.
    #[serde(default)]
    pub source: Value,
    /// Validated merchant key/value metadata.
    #[serde(default)]
    pub metadata: Metadata,
    /// Optional marketplace split allocations.
    #[serde(default)]
    pub splits: Option<Vec<Value>>,
    /// Optional 3DS / redirect callback URL.
    #[serde(default)]
    pub callback_url: Option<String>,
    /// Optional associated invoice identifier.
    #[serde(default)]
    pub invoice_id: Option<String>,
    /// Optional payer IP address (redacted in `Debug` output).
    #[serde(default)]
    pub ip: Option<String>,
    /// Timestamp when the payment was captured, if applicable.
    #[serde(default)]
    pub captured_at: Option<DateTime<Utc>>,
    /// Timestamp when the payment was refunded, if applicable.
    #[serde(default)]
    pub refunded_at: Option<DateTime<Utc>>,
    /// Timestamp when the payment was voided, if applicable.
    #[serde(default)]
    pub voided_at: Option<DateTime<Utc>>,
    /// Timestamp when the payment was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when the payment was last updated.
    pub updated_at: DateTime<Utc>,
}

impl fmt::Debug for Payment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Payment")
            .field("id", &self.id)
            .field("status", &self.status)
            .field("amount", &self.amount)
            .field("amount_format", &self.amount_format)
            .field("currency", &self.currency)
            .field("metadata", &self.metadata)
            .field("ip", &self.ip.as_ref().map(|_| "[REDACTED]"))
            .field("created_at", &self.created_at)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tc_2_1_0_02_dual_field_deserialization_and_timestamps() {
        let payload = json!({
            "id": "pay_01J9XYZ",
            "status": "paid",
            "amount": 15000,
            "amount_format": "150.00 SAR",
            "fee": 350,
            "fee_format": "3.50 SAR",
            "refunded": 0,
            "refunded_format": "0.00 SAR",
            "captured": 15000,
            "captured_format": "150.00 SAR",
            "currency": "SAR",
            "description": "Order #1042",
            "source": { "type": "creditcard", "company": "mada" },
            "metadata": { "order_id": "ORD-1042" },
            "callback_url": "https://merchant.example/callback",
            "invoice_id": null,
            "ip": "192.0.2.10",
            "captured_at": "2026-11-13T10:01:00Z",
            "refunded_at": null,
            "voided_at": null,
            "created_at": "2026-11-13T10:00:00Z",
            "updated_at": "2026-11-13T10:01:00Z"
        });

        let payment: Payment = serde_json::from_value(payload).expect("valid payment");
        assert_eq!(payment.id, "pay_01J9XYZ");
        assert_eq!(payment.status, PaymentStatus::Paid);
        assert_eq!(payment.amount, 15000);
        assert_eq!(payment.amount_format, "150.00 SAR");
        assert_eq!(payment.fee, 350);
        assert_eq!(payment.fee_format, "3.50 SAR");
        assert_eq!(payment.refunded, 0);
        assert_eq!(payment.refunded_format, "0.00 SAR");
        assert_eq!(payment.captured, 15000);
        assert_eq!(payment.captured_format, "150.00 SAR");
        assert_eq!(payment.currency, Currency::SAR);
        assert_eq!(payment.metadata.get("order_id"), Some("ORD-1042"));
        assert_eq!(
            payment.callback_url.as_deref(),
            Some("https://merchant.example/callback")
        );
        assert_eq!(payment.ip.as_deref(), Some("192.0.2.10"));
        assert!(payment.captured_at.is_some());
        assert!(payment.refunded_at.is_none());
    }

    #[test]
    fn payment_status_roundtrip_and_unknown_variant() {
        for (wire, expected) in KNOWN_STATUSES {
            let parsed: PaymentStatus =
                serde_json::from_str(&format!("\"{wire}\"")).expect("valid status");
            assert_eq!(&parsed, expected);
            assert_eq!(parsed.as_str(), *wire);
        }

        let future: PaymentStatus =
            serde_json::from_str("\"partially_refunded\"").expect("unknown status");
        assert_eq!(
            future,
            PaymentStatus::Unknown("partially_refunded".to_owned())
        );
        assert_eq!(future.as_str(), "partially_refunded");
    }

    #[test]
    fn payment_debug_redacts_metadata_values_and_ip() {
        let payload = json!({
            "id": "pay_secret_debug",
            "status": "initiated",
            "amount": 1000,
            "amount_format": "10.00",
            "fee": 0,
            "fee_format": "0.00",
            "refunded": 0,
            "refunded_format": "0.00",
            "captured": 0,
            "captured_format": "0.00",
            "currency": "SAR",
            "metadata": { "customer_email": "pii@example.com" },
            "ip": "203.0.113.42",
            "created_at": "2026-11-13T10:00:00Z",
            "updated_at": "2026-11-13T10:00:00Z"
        });
        let payment: Payment = serde_json::from_value(payload).expect("valid payment");
        let dbg = format!("{payment:?}");
        assert!(!dbg.contains("pii@example.com"));
        assert!(!dbg.contains("203.0.113.42"));
        assert!(dbg.contains("[REDACTED]"));
    }
}
