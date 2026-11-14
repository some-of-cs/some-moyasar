//! Aggregation transfer domain models (`Transfer`, `TransferLine`, `TransferId`).

use core::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kernel::error::MoyasarError;
use crate::kernel::types::{Amount, Currency};
use crate::resources::settlements::models::{SettlementRecipientType, validate_path_segment_id};

/// Strongly-typed identifier for a Moyasar aggregation transfer.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TransferId(
    /// Raw transfer identifier string.
    pub String,
);

impl TransferId {
    /// Creates a new [`TransferId`] from any string-like value.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the underlying transfer identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Validates that this transfer identifier is a non-empty, URL-safe path segment.
    pub fn validate(&self) -> Result<(), MoyasarError> {
        validate_path_segment_id(&self.0, "transfer id")
    }
}

impl fmt::Display for TransferId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AsRef<str> for TransferId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<&str> for TransferId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Moyasar aggregation transfer resource returned by `/v1/transfers` endpoints.
///
/// # Examples
///
/// ```
/// use some_moyasar::kernel::types::{Amount, Currency};
/// use some_moyasar::resources::settlements::transfer_models::{Transfer, TransferId};
///
/// let payload = r#"{
///     "id": "tr_01J9X2K3M4N5P6Q7R8S9T0",
///     "amount": 150000,
///     "fee": 2500,
///     "tax": 375,
///     "currency": "SAR",
///     "reference": "batch-2026-11",
///     "created_at": "2026-11-01T10:00:00Z"
/// }"#;
///
/// let transfer: Transfer = serde_json::from_str(payload).unwrap();
/// assert_eq!(transfer.id, TransferId::new("tr_01J9X2K3M4N5P6Q7R8S9T0"));
/// assert_eq!(transfer.amount, Amount::from_minor(150_000));
/// assert_eq!(transfer.fee, Some(Amount::from_minor(2_500)));
/// assert_eq!(transfer.tax, Some(Amount::from_minor(375)));
/// assert_eq!(transfer.currency, Currency::SAR);
/// assert_eq!(transfer.reference.as_deref(), Some("batch-2026-11"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transfer {
    /// Unique identifier for the transfer.
    pub id: TransferId,
    /// Classification of the transfer recipient, if provided.
    #[serde(default)]
    pub recipient_type: Option<SettlementRecipientType>,
    /// Unique UUID of the transfer recipient, if provided.
    #[serde(default)]
    pub recipient_id: Option<Uuid>,
    /// Net or gross transfer amount in the smallest currency unit.
    pub amount: Amount,
    /// Processing fee charged on the transfer in minor units, if present.
    #[serde(default)]
    pub fee: Option<Amount>,
    /// Tax applied to the transfer fee in minor units, if present.
    #[serde(default)]
    pub tax: Option<Amount>,
    /// ISO 4217 currency code for the transfer amounts.
    pub currency: Currency,
    /// Optional merchant or reconciliation reference for the transfer.
    #[serde(default)]
    pub reference: Option<String>,
    /// Timestamp when the transfer was created.
    pub created_at: DateTime<Utc>,
}

/// Individual ledger line item belonging to an aggregation [`Transfer`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferLine {
    /// Unique UUID identifier of the transfer line item, if provided.
    #[serde(default)]
    pub id: Option<Uuid>,
    /// Associated internal transaction UUID, if applicable.
    #[serde(default)]
    pub internal_transaction_id: Option<Uuid>,
    /// Associated payment UUID, if applicable.
    #[serde(default)]
    pub payment_id: Option<Uuid>,
    /// Line item amount in the smallest currency unit.
    pub amount: Amount,
    /// Processing fee for this line item in minor units, if present.
    #[serde(default)]
    pub fee: Option<Amount>,
    /// Tax applied to this line item's fee in minor units, if present.
    #[serde(default)]
    pub tax: Option<Amount>,
    /// ISO 4217 currency code for this line item.
    pub currency: Currency,
    /// Timestamp when the transfer line item was created.
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_id_conversions_and_transparent_serde() {
        let from_new = TransferId::new("tr_123");
        let from_str: TransferId = "tr_123".into();

        assert_eq!(from_new, from_str);
        assert_eq!(from_new.as_str(), "tr_123");
        assert_eq!(from_new.as_ref(), "tr_123");
        assert_eq!(from_new.to_string(), "tr_123");

        let json = serde_json::to_string(&from_new).expect("serializes transparently");
        assert_eq!(json, "\"tr_123\"");

        let parsed: TransferId = serde_json::from_str(&json).expect("deserializes transparently");
        assert_eq!(parsed, from_new);
    }

    #[test]
    fn transfer_deserializes_full_and_minimal_payloads() {
        let full_raw = r#"{
            "id": "tr_01J9X2K3M4N5P6Q7R8S9T0",
            "recipient_type": null,
            "recipient_id": "550e8400-e29b-41d4-a716-446655440000",
            "amount": 250000,
            "fee": 3500,
            "tax": 525,
            "currency": "SAR",
            "reference": "settlement-ref-42",
            "created_at": "2026-11-01T10:00:00Z"
        }"#;

        let full: Transfer = serde_json::from_str(full_raw).expect("valid full transfer JSON");
        assert_eq!(full.id, TransferId::from("tr_01J9X2K3M4N5P6Q7R8S9T0"));
        assert!(full.recipient_type.is_none());
        assert_eq!(
            full.recipient_id,
            Some(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap())
        );
        assert_eq!(full.amount, Amount::from_minor(250_000));
        assert_eq!(full.fee, Some(Amount::from_minor(3_500)));
        assert_eq!(full.tax, Some(Amount::from_minor(525)));
        assert_eq!(full.currency, Currency::SAR);
        assert_eq!(full.reference.as_deref(), Some("settlement-ref-42"));
        assert_eq!(full.created_at.to_rfc3339(), "2026-11-01T10:00:00+00:00");

        let serialized = serde_json::to_string(&full).expect("serializes cleanly");
        let round_tripped: Transfer =
            serde_json::from_str(&serialized).expect("round-trips cleanly");
        assert_eq!(round_tripped, full);

        let minimal_raw = r#"{
            "id": "tr_minimal",
            "amount": 1000,
            "currency": "KWD",
            "created_at": "2026-11-02T12:30:00Z"
        }"#;

        let minimal: Transfer =
            serde_json::from_str(minimal_raw).expect("valid minimal transfer JSON");
        assert_eq!(minimal.id.as_str(), "tr_minimal");
        assert!(minimal.recipient_type.is_none());
        assert!(minimal.recipient_id.is_none());
        assert_eq!(minimal.amount, Amount::from_major(1, Currency::KWD));
        assert!(minimal.fee.is_none());
        assert!(minimal.tax.is_none());
        assert_eq!(minimal.currency, Currency::KWD);
        assert!(minimal.reference.is_none());
    }

    #[test]
    fn transfer_line_deserializes_full_and_minimal_payloads() {
        let full_raw = r#"{
            "id": "11111111-1111-4111-8111-111111111111",
            "internal_transaction_id": "22222222-2222-4222-8222-222222222222",
            "payment_id": "33333333-3333-4333-8333-333333333333",
            "amount": 50000,
            "fee": 1000,
            "tax": 150,
            "currency": "SAR",
            "created_at": "2026-11-01T11:15:00Z"
        }"#;

        let full: TransferLine =
            serde_json::from_str(full_raw).expect("valid full transfer line JSON");
        assert_eq!(
            full.id,
            Some(Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap())
        );
        assert_eq!(
            full.internal_transaction_id,
            Some(Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap())
        );
        assert_eq!(
            full.payment_id,
            Some(Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap())
        );
        assert_eq!(full.amount, Amount::from_minor(50_000));
        assert_eq!(full.fee, Some(Amount::from_minor(1_000)));
        assert_eq!(full.tax, Some(Amount::from_minor(150)));
        assert_eq!(full.currency, Currency::SAR);
        assert_eq!(full.created_at.to_rfc3339(), "2026-11-01T11:15:00+00:00");

        let serialized = serde_json::to_string(&full).expect("serializes cleanly");
        let round_tripped: TransferLine =
            serde_json::from_str(&serialized).expect("round-trips cleanly");
        assert_eq!(round_tripped, full);

        let minimal_raw = r#"{
            "amount": 100,
            "currency": "USD",
            "created_at": "2026-11-03T08:00:00Z"
        }"#;

        let minimal: TransferLine =
            serde_json::from_str(minimal_raw).expect("valid minimal transfer line JSON");
        assert!(minimal.id.is_none());
        assert!(minimal.internal_transaction_id.is_none());
        assert!(minimal.payment_id.is_none());
        assert_eq!(minimal.amount, Amount::from_major(1, Currency::USD));
        assert!(minimal.fee.is_none());
        assert!(minimal.tax.is_none());
        assert_eq!(minimal.currency, Currency::USD);
    }
}
