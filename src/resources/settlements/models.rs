//! Settlement domain models (`Settlement`, `SettlementId`, `SettlementRecipientType`).

use core::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::types::{Amount, Currency};

const RECIPIENT_TYPE_LABELS: &[(SettlementRecipientType, &str)] = &[
    (SettlementRecipientType::Entity, "entity"),
    (SettlementRecipientType::Platform, "platform"),
    (SettlementRecipientType::Beneficiary, "beneficiary"),
    (SettlementRecipientType::Unknown, "unknown"),
];

/// Strongly typed identifier for a Moyasar settlement (e.g., `stl_01J9XYZ`).
///
/// Serializes transparently as a JSON string.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::settlements::models::SettlementId;
///
/// let id = SettlementId::new("stl_01J9XYZ");
/// assert_eq!(id.as_str(), "stl_01J9XYZ");
/// assert_eq!(id.to_string(), "stl_01J9XYZ");
///
/// let from_ref: SettlementId = "stl_01J9ABC".into();
/// assert_eq!(from_ref.as_ref(), "stl_01J9ABC");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SettlementId(pub String);

impl SettlementId {
    /// Creates a new [`SettlementId`] from any string-like value.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the underlying settlement identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Validates that this settlement identifier is a non-empty, URL-safe path segment.
    pub fn validate(&self) -> Result<(), MoyasarError> {
        validate_path_segment_id(&self.0, "settlement id")
    }
}

pub(crate) fn validate_path_segment_id(raw: &str, label: &str) -> Result<(), MoyasarError> {
    let invalid = raw.is_empty()
        || raw.contains("..")
        || raw
            .chars()
            .any(|c| c.is_whitespace() || "/?#\\".contains(c));
    if invalid {
        return Err(MoyasarError::Api {
            error_type: ApiErrorType::InvalidRequestError,
            message: format!("{label} must be a non-empty URL-safe path segment"),
            errors: Vec::new(),
            response_code: None,
        });
    }
    Ok(())
}

impl fmt::Display for SettlementId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for SettlementId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<&str> for SettlementId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl From<String> for SettlementId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// Classification of the recipient receiving a Moyasar settlement payout.
///
/// Both `"entity"` and `"merchant"` wire strings deserialize into
/// [`SettlementRecipientType::Entity`]. Unrecognized future recipient types
/// returned by the API deserialize into [`SettlementRecipientType::Unknown`]
/// for forward compatibility.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::settlements::models::SettlementRecipientType;
///
/// let entity: SettlementRecipientType = serde_json::from_str(r#""entity""#).unwrap();
/// assert_eq!(entity, SettlementRecipientType::Entity);
/// assert_eq!(entity.as_str(), "entity");
///
/// let merchant: SettlementRecipientType = serde_json::from_str(r#""merchant""#).unwrap();
/// assert_eq!(merchant, SettlementRecipientType::Entity);
///
/// let future: SettlementRecipientType = serde_json::from_str(r#""partner_bank""#).unwrap();
/// assert_eq!(future, SettlementRecipientType::Unknown);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettlementRecipientType {
    /// Standard merchant entity settlement (also accepts `"merchant"` alias on wire).
    #[serde(alias = "merchant")]
    Entity,
    /// Marketplace or platform fee settlement.
    Platform,
    /// Third-party beneficiary settlement.
    Beneficiary,
    /// Forward-compatible fallback for unrecognized recipient types.
    #[serde(other)]
    Unknown,
}

impl SettlementRecipientType {
    /// Returns the lowercase wire representation of this recipient type.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        RECIPIENT_TYPE_LABELS
            .iter()
            .find(|&&(kind, _)| kind == self)
            .map_or("unknown", |&(_, label)| label)
    }
}

impl fmt::Display for SettlementRecipientType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Moyasar Settlement resource returned by `GET /v1/settlements` and
/// `GET /v1/settlements/:id`.
///
/// Monetary fields (`amount`, `fee`, `tax`, `invoicing_fee`, `invoicing_tax`)
/// are stored in the smallest currency minor unit via [`Amount`].
///
/// `fmt::Debug` redacts `invoice_url`, `csv_list_url`, and `pdf_list_url` to
/// `"[REDACTED]"` when present to prevent signed report URL leakage in logs.
///
/// # Examples
///
/// ```
/// use some_moyasar::kernel::types::{Amount, Currency};
/// use some_moyasar::resources::settlements::models::{
///     Settlement, SettlementId, SettlementRecipientType,
/// };
///
/// let payload = r#"{
///     "id": "stl_01J9XYZ",
///     "recipient_type": "entity",
///     "currency": "SAR",
///     "source_currency": "SAR",
///     "amount": 250000,
///     "fee": 4500,
///     "tax": 675,
///     "reference": "STL-2026-11-01",
///     "settlement_count": 42,
///     "invoice_url": "https://files.moyasar.com/signed/invoice.pdf?sig=secret",
///     "created_at": "2026-11-14T00:00:00Z"
/// }"#;
///
/// let settlement: Settlement = serde_json::from_str(payload).unwrap();
/// assert_eq!(settlement.id, SettlementId::new("stl_01J9XYZ"));
/// assert_eq!(settlement.recipient_type, SettlementRecipientType::Entity);
/// assert_eq!(settlement.currency, Currency::SAR);
/// assert_eq!(settlement.amount, Amount::from_minor(250_000));
///
/// let debug_output = format!("{settlement:?}");
/// assert!(debug_output.contains("[REDACTED]"));
/// assert!(!debug_output.contains("sig=secret"));
/// ```
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Settlement {
    /// Unique Moyasar settlement identifier.
    pub id: SettlementId,
    /// Classification of the settlement recipient.
    pub recipient_type: SettlementRecipientType,
    /// Settlement payout ISO 4217 currency.
    pub currency: Currency,
    /// Original transaction ISO 4217 currency prior to FX conversion.
    pub source_currency: Currency,
    /// Optional invoicing ISO 4217 currency when billed in a separate currency.
    #[serde(default)]
    pub invoicing_currency: Option<Currency>,
    /// Gross settlement amount in minor units of [`Self::currency`].
    pub amount: Amount,
    /// Total processing fee deducted in minor units of [`Self::currency`].
    pub fee: Amount,
    /// Total VAT/tax deducted in minor units of [`Self::currency`].
    pub tax: Amount,
    /// Optional fee expressed in [`Self::invoicing_currency`] minor units.
    #[serde(default)]
    pub invoicing_fee: Option<Amount>,
    /// Optional tax expressed in [`Self::invoicing_currency`] minor units.
    #[serde(default)]
    pub invoicing_tax: Option<Amount>,
    /// Optional foreign-exchange conversion rate applied for invoicing.
    #[serde(default)]
    pub invoicing_ex_rate: Option<f64>,
    /// Optional merchant-facing settlement batch reference code.
    #[serde(default)]
    pub reference: Option<String>,
    /// Optional count of settled transactions included in this batch.
    #[serde(default)]
    pub settlement_count: Option<u32>,
    /// Optional pre-signed URL for the settlement tax invoice PDF.
    #[serde(default)]
    pub invoice_url: Option<String>,
    /// Optional pre-signed URL for the settlement line-items CSV export.
    #[serde(default)]
    pub csv_list_url: Option<String>,
    /// Optional pre-signed URL for the settlement line-items PDF export.
    #[serde(default)]
    pub pdf_list_url: Option<String>,
    /// UTC timestamp when the settlement was created.
    pub created_at: DateTime<Utc>,
}

impl fmt::Debug for Settlement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Settlement")
            .field("id", &self.id)
            .field("recipient_type", &self.recipient_type)
            .field("currency", &self.currency)
            .field("source_currency", &self.source_currency)
            .field("invoicing_currency", &self.invoicing_currency)
            .field("amount", &self.amount)
            .field("fee", &self.fee)
            .field("tax", &self.tax)
            .field("invoicing_fee", &self.invoicing_fee)
            .field("invoicing_tax", &self.invoicing_tax)
            .field("invoicing_ex_rate", &self.invoicing_ex_rate)
            .field("reference", &self.reference)
            .field("settlement_count", &self.settlement_count)
            .field(
                "invoice_url",
                &self.invoice_url.as_ref().map(|_| "[REDACTED]"),
            )
            .field(
                "csv_list_url",
                &self.csv_list_url.as_ref().map(|_| "[REDACTED]"),
            )
            .field(
                "pdf_list_url",
                &self.pdf_list_url.as_ref().map(|_| "[REDACTED]"),
            )
            .field("created_at", &self.created_at)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tc_6_1_0_03_settlement_deserialization_and_id_traits() {
        let id = SettlementId::new("stl_01J9XYZ");
        assert_eq!(id.as_str(), "stl_01J9XYZ");
        assert_eq!(id.to_string(), "stl_01J9XYZ");
        assert_eq!(AsRef::<str>::as_ref(&id), "stl_01J9XYZ");
        assert_eq!(SettlementId::from("stl_01J9XYZ"), id);

        let payload = json!({
            "id": "stl_01J9XYZ",
            "recipient_type": "entity",
            "currency": "SAR",
            "source_currency": "SAR",
            "amount": 500_000,
            "fee": 12_500,
            "tax": 1_875,
            "reference": "STL-2026-11-14",
            "settlement_count": 128,
            "invoice_url": "https://files.moyasar.com/inv.pdf?token=abc",
            "csv_list_url": "https://files.moyasar.com/list.csv?token=def",
            "pdf_list_url": "https://files.moyasar.com/list.pdf?token=ghi",
            "created_at": "2026-11-14T03:00:00Z"
        });

        let settlement: Settlement = serde_json::from_value(payload).unwrap();
        assert_eq!(settlement.id, id);
        assert_eq!(settlement.recipient_type, SettlementRecipientType::Entity);
        assert_eq!(settlement.currency, Currency::SAR);
        assert_eq!(settlement.source_currency, Currency::SAR);
        assert_eq!(settlement.amount, Amount::from_minor(500_000));
        assert_eq!(settlement.fee, Amount::from_minor(12_500));
        assert_eq!(settlement.tax, Amount::from_minor(1_875));
        assert_eq!(settlement.reference.as_deref(), Some("STL-2026-11-14"));
        assert_eq!(settlement.settlement_count, Some(128));
        assert_eq!(
            settlement.invoice_url.as_deref(),
            Some("https://files.moyasar.com/inv.pdf?token=abc")
        );
    }

    #[test]
    fn tc_6_1_0_04_optional_fields_and_unknown_recipient_type() {
        let payload = json!({
            "id": "stl_01J9MIN",
            "recipient_type": "marketplace_escrow",
            "currency": "SAR",
            "source_currency": "SAR",
            "amount": 10_000,
            "fee": 250,
            "tax": 38,
            "created_at": "2026-11-14T04:00:00Z"
        });

        let settlement: Settlement = serde_json::from_value(payload).unwrap();
        assert_eq!(settlement.recipient_type, SettlementRecipientType::Unknown);
        assert_eq!(settlement.recipient_type.as_str(), "unknown");
        assert!(settlement.invoicing_currency.is_none());
        assert!(settlement.invoicing_fee.is_none());
        assert!(settlement.invoicing_tax.is_none());
        assert!(settlement.invoicing_ex_rate.is_none());
        assert!(settlement.reference.is_none());
        assert!(settlement.settlement_count.is_none());
        assert!(settlement.invoice_url.is_none());
        assert!(settlement.csv_list_url.is_none());
        assert!(settlement.pdf_list_url.is_none());

        let merchant: SettlementRecipientType = serde_json::from_str(r#""merchant""#).unwrap();
        let platform: SettlementRecipientType = serde_json::from_str(r#""platform""#).unwrap();
        let beneficiary: SettlementRecipientType =
            serde_json::from_str(r#""beneficiary""#).unwrap();
        assert_eq!(merchant, SettlementRecipientType::Entity);
        assert_eq!(merchant.as_str(), "entity");
        assert_eq!(platform, SettlementRecipientType::Platform);
        assert_eq!(beneficiary, SettlementRecipientType::Beneficiary);
    }

    #[test]
    fn debug_redacts_signed_report_urls_when_present_and_preserves_none() {
        let with_urls: Settlement = serde_json::from_value(json!({
            "id": "stl_01J9RED",
            "recipient_type": "platform",
            "currency": "SAR",
            "source_currency": "SAR",
            "amount": 100_000,
            "fee": 2_000,
            "tax": 300,
            "invoice_url": "https://signed.example/invoice?sig=secret1",
            "csv_list_url": "https://signed.example/list.csv?sig=secret2",
            "pdf_list_url": "https://signed.example/list.pdf?sig=secret3",
            "created_at": "2026-11-14T05:00:00Z"
        }))
        .unwrap();

        let dbg_with_urls = format!("{with_urls:?}");
        assert!(dbg_with_urls.contains("invoice_url: Some(\"[REDACTED]\")"));
        assert!(dbg_with_urls.contains("csv_list_url: Some(\"[REDACTED]\")"));
        assert!(dbg_with_urls.contains("pdf_list_url: Some(\"[REDACTED]\")"));
        assert!(!dbg_with_urls.contains("secret1"));
        assert!(!dbg_with_urls.contains("secret2"));
        assert!(!dbg_with_urls.contains("secret3"));

        let without_urls: Settlement = serde_json::from_value(json!({
            "id": "stl_01J9NONE",
            "recipient_type": "beneficiary",
            "currency": "SAR",
            "source_currency": "SAR",
            "amount": 50_000,
            "fee": 1_000,
            "tax": 150,
            "created_at": "2026-11-14T05:00:00Z"
        }))
        .unwrap();

        let dbg_without_urls = format!("{without_urls:?}");
        assert!(dbg_without_urls.contains("invoice_url: None"));
        assert!(dbg_without_urls.contains("csv_list_url: None"));
        assert!(dbg_without_urls.contains("pdf_list_url: None"));
    }

    #[test]
    fn multi_currency_fx_deserialization() {
        let payload = json!({
            "id": "stl_01J9FX",
            "recipient_type": "entity",
            "currency": "USD",
            "source_currency": "SAR",
            "invoicing_currency": "SAR",
            "amount": 26_667,
            "fee": 800,
            "tax": 120,
            "invoicing_fee": 3_000,
            "invoicing_tax": 450,
            "invoicing_ex_rate": 0.26667,
            "created_at": "2026-11-14T05:30:00Z"
        });

        let settlement: Settlement = serde_json::from_value(payload).unwrap();
        assert_eq!(settlement.currency, Currency::USD);
        assert_eq!(settlement.source_currency, Currency::SAR);
        assert_eq!(settlement.invoicing_currency, Some(Currency::SAR));
        assert_eq!(settlement.invoicing_fee, Some(Amount::from_minor(3_000)));
        assert_eq!(settlement.invoicing_tax, Some(Amount::from_minor(450)));
        assert_eq!(settlement.invoicing_ex_rate, Some(0.26667));
    }
}
