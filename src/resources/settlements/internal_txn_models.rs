//! Internal transaction domain models (`InternalTransaction`, `InternalTransactionId`).

use core::fmt;
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kernel::types::{Amount, Currency};
use crate::resources::settlements::models::SettlementRecipientType;

/// Strongly-typed identifier for a Moyasar internal transaction.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InternalTransactionId(pub String);

impl InternalTransactionId {
    /// Creates a new [`InternalTransactionId`] from any string-like value.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the underlying identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InternalTransactionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for InternalTransactionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<&str> for InternalTransactionId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

/// Moyasar internal ledger transaction awaiting or settled into a transfer.
///
/// # Examples
///
/// ```rust
/// use some_moyasar::resources::settlements::internal_txn_models::InternalTransaction;
/// use some_moyasar::resources::settlements::models::SettlementRecipientType;
///
/// let payload = r#"{
///     "id": "itxn_01J9XYZ",
///     "recipient_type": "entity",
///     "recipient_id": "550e8400-e29b-41d4-a716-446655440000",
///     "amount": 15000,
///     "currency": "SAR",
///     "transfer_id": null,
///     "settled_at": null,
///     "created_at": "2026-11-13T10:00:00Z",
///     "updated_at": "2026-11-13T10:00:00Z",
///     "metadata": null
/// }"#;
///
/// let txn: InternalTransaction = serde_json::from_str(payload).unwrap();
/// assert_eq!(txn.id.as_str(), "itxn_01J9XYZ");
/// assert_eq!(txn.recipient_type, SettlementRecipientType::Entity);
/// assert!(!txn.is_settled());
/// ```
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InternalTransaction {
    /// Unique Moyasar internal transaction identifier.
    pub id: InternalTransactionId,
    /// Classification of the settlement recipient.
    pub recipient_type: SettlementRecipientType,
    /// Unique UUID of the settlement recipient.
    pub recipient_id: Uuid,
    /// Transaction amount in the currency's smallest minor unit.
    pub amount: Amount,
    /// ISO 4217 currency code.
    pub currency: Currency,
    /// Associated aggregation transfer identifier once settled.
    #[serde(default)]
    pub transfer_id: Option<String>,
    /// Timestamp when the transaction was settled into a transfer.
    #[serde(default)]
    pub settled_at: Option<DateTime<Utc>>,
    /// Timestamp when the internal transaction was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when the internal transaction was last updated.
    pub updated_at: DateTime<Utc>,
    /// Optional key-value metadata (redacted in `Debug` output).
    #[serde(default)]
    pub metadata: Option<BTreeMap<String, String>>,
}

impl fmt::Debug for InternalTransaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redacted_metadata = self.metadata.as_ref().map(|_| "[REDACTED]");
        f.debug_struct("InternalTransaction")
            .field("id", &self.id)
            .field("recipient_type", &self.recipient_type)
            .field("recipient_id", &self.recipient_id)
            .field("amount", &self.amount)
            .field("currency", &self.currency)
            .field("transfer_id", &self.transfer_id)
            .field("settled_at", &self.settled_at)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .field("metadata", &redacted_metadata)
            .finish()
    }
}

impl InternalTransaction {
    /// Returns `true` when both `transfer_id` and `settled_at` are populated.
    pub const fn is_settled(&self) -> bool {
        self.transfer_id.is_some() && self.settled_at.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn internal_transaction_id_traits_and_serde() {
        let id = InternalTransactionId::new("itxn_01");
        assert_eq!(id.as_str(), "itxn_01");
        assert_eq!(id.as_ref(), "itxn_01");
        assert_eq!(id.to_string(), "itxn_01");
        assert_eq!(InternalTransactionId::from("itxn_01"), id);

        let serialized = serde_json::to_string(&id).expect("serialize id");
        assert_eq!(serialized, "\"itxn_01\"");
    }

    #[test]
    fn tc_6_2_0_04_unsettled_internal_transaction_has_none_fields() {
        let payload = json!({
            "id": "itxn_unsettled_04",
            "recipient_type": "entity",
            "recipient_id": "550e8400-e29b-41d4-a716-446655440000",
            "amount": 25000,
            "currency": "SAR",
            "transfer_id": null,
            "settled_at": null,
            "created_at": "2026-11-13T10:00:00Z",
            "updated_at": "2026-11-13T10:00:00Z"
        });

        let txn: InternalTransaction =
            serde_json::from_value(payload).expect("valid unsettled internal transaction");
        assert_eq!(txn.id.as_str(), "itxn_unsettled_04");
        assert_eq!(txn.recipient_type, SettlementRecipientType::Entity);
        assert_eq!(txn.amount, Amount::from_minor(25000));
        assert_eq!(txn.currency, Currency::SAR);
        assert!(txn.transfer_id.is_none());
        assert!(txn.settled_at.is_none());
        assert!(txn.metadata.is_none());
        assert!(!txn.is_settled());
    }

    #[test]
    fn tc_6_2_0_05_settled_internal_transaction_has_some_fields() {
        let payload = json!({
            "id": "itxn_settled_05",
            "recipient_type": "entity",
            "recipient_id": "550e8400-e29b-41d4-a716-446655440000",
            "amount": 50000,
            "currency": "SAR",
            "transfer_id": "tr_01J9ABC",
            "settled_at": "2026-11-13T12:00:00Z",
            "created_at": "2026-11-13T10:00:00Z",
            "updated_at": "2026-11-13T12:00:00Z",
            "metadata": { "batch": "daily" }
        });

        let txn: InternalTransaction =
            serde_json::from_value(payload).expect("valid settled internal transaction");
        assert_eq!(txn.recipient_type, SettlementRecipientType::Entity);
        assert_eq!(txn.transfer_id.as_deref(), Some("tr_01J9ABC"));
        assert!(txn.settled_at.is_some());
        assert!(txn.is_settled());

        let round_trip: InternalTransaction = serde_json::from_str(
            &serde_json::to_string(&txn).expect("serializes settled internal transaction"),
        )
        .expect("round-trips settled internal transaction");
        assert_eq!(round_trip, txn);
    }

    #[test]
    fn debug_redacts_metadata_when_present() {
        let payload = json!({
            "id": "itxn_debug_01",
            "recipient_type": "entity",
            "recipient_id": "550e8400-e29b-41d4-a716-446655440000",
            "amount": 1000,
            "currency": "SAR",
            "created_at": "2026-11-13T10:00:00Z",
            "updated_at": "2026-11-13T10:00:00Z",
            "metadata": { "secret_key": "sensitive_value_123" }
        });

        let txn: InternalTransaction =
            serde_json::from_value(payload).expect("valid internal transaction");
        assert_eq!(txn.recipient_type, SettlementRecipientType::Entity);
        let dbg = format!("{txn:?}");
        assert!(!dbg.contains("secret_key"));
        assert!(!dbg.contains("sensitive_value_123"));
        assert!(dbg.contains("[REDACTED]"));
    }

    #[test]
    fn recipient_type_merchant_alias_unknown_fallback_and_transfer_id_formats() {
        let merchant_payload = json!({
            "id": "itxn_merchant_alias",
            "recipient_type": "merchant",
            "recipient_id": "550e8400-e29b-41d4-a716-446655440000",
            "amount": 30000,
            "currency": "SAR",
            "transfer_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
            "settled_at": "2026-11-13T14:00:00Z",
            "created_at": "2026-11-13T10:00:00Z",
            "updated_at": "2026-11-13T14:00:00Z"
        });

        let merchant_txn: InternalTransaction =
            serde_json::from_value(merchant_payload).expect("merchant alias deserializes");
        assert_eq!(merchant_txn.recipient_type, SettlementRecipientType::Entity);
        assert_eq!(
            merchant_txn.transfer_id.as_deref(),
            Some("a1b2c3d4-e5f6-7890-abcd-ef1234567890")
        );
        assert!(merchant_txn.is_settled());

        let unknown_payload = json!({
            "id": "itxn_unknown_recipient",
            "recipient_type": "sub_account_future",
            "recipient_id": "550e8400-e29b-41d4-a716-446655440000",
            "amount": 12000,
            "currency": "SAR",
            "transfer_id": "tr_01J9XYZ",
            "settled_at": "2026-11-13T15:00:00Z",
            "created_at": "2026-11-13T10:00:00Z",
            "updated_at": "2026-11-13T15:00:00Z"
        });

        let unknown_txn: InternalTransaction =
            serde_json::from_value(unknown_payload).expect("unknown recipient_type deserializes");
        assert_eq!(
            unknown_txn.recipient_type,
            SettlementRecipientType::Unknown
        );
        assert_eq!(unknown_txn.transfer_id.as_deref(), Some("tr_01J9XYZ"));
    }
}
