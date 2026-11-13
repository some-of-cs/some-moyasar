//! Invoice domain models (`Invoice`, `InvoiceStatus`).

use core::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kernel::metadata::Metadata;
use crate::kernel::types::{Amount, Currency};

const STATUS_LABELS: &[(InvoiceStatus, &str)] = &[
    (InvoiceStatus::Created, "created"),
    (InvoiceStatus::Paid, "paid"),
    (InvoiceStatus::Canceled, "canceled"),
    (InvoiceStatus::Expired, "expired"),
    (InvoiceStatus::Unknown, "unknown"),
];

/// Lifecycle status of a Moyasar hosted e-invoice.
///
/// Unrecognized future statuses returned by the API deserialize to
/// [`InvoiceStatus::Unknown`] for forward compatibility.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::invoices::models::InvoiceStatus;
///
/// let paid: InvoiceStatus = serde_json::from_str(r#""paid""#).unwrap();
/// assert_eq!(paid, InvoiceStatus::Paid);
/// assert_eq!(paid.as_str(), "paid");
///
/// let future: InvoiceStatus = serde_json::from_str(r#""pending_review""#).unwrap();
/// assert_eq!(future, InvoiceStatus::Unknown);
/// assert_eq!(future.as_str(), "unknown");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InvoiceStatus {
    /// Invoice has been created and is awaiting payment.
    Created,
    /// Invoice has been paid in full.
    Paid,
    /// Invoice was canceled via `PUT /v1/invoices/:id/cancel` (terminal).
    Canceled,
    /// Invoice passed its `expired_at` timestamp without payment.
    Expired,
    /// Forward-compatible fallback for unrecognized status values.
    #[serde(other)]
    Unknown,
}

impl InvoiceStatus {
    /// Returns the lowercase wire representation of this invoice status.
    ///
    /// # Examples
    ///
    /// ```
    /// use some_moyasar::resources::invoices::models::InvoiceStatus;
    ///
    /// assert_eq!(InvoiceStatus::Created.as_str(), "created");
    /// assert_eq!(InvoiceStatus::Canceled.as_str(), "canceled");
    /// ```
    #[must_use]
    pub fn as_str(self) -> &'static str {
        STATUS_LABELS
            .iter()
            .find(|&&(status, _)| status == self)
            .map_or("unknown", |&(_, label)| label)
    }
}

impl fmt::Display for InvoiceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Moyasar hosted e-invoice resource returned by `/v1/invoices` endpoints.
///
/// # Examples
///
/// ```
/// use some_moyasar::kernel::types::{Amount, Currency};
/// use some_moyasar::resources::invoices::models::{Invoice, InvoiceStatus};
///
/// let payload = r#"{
///     "id": "550e8400-e29b-41d4-a716-446655440000",
///     "status": "created",
///     "amount": 50000,
///     "currency": "SAR",
///     "description": "Annual subscription",
///     "url": "https://moyasar.com/i/550e8400-e29b-41d4-a716-446655440000"
/// }"#;
///
/// let invoice: Invoice = serde_json::from_str(payload).unwrap();
/// assert_eq!(invoice.status, InvoiceStatus::Created);
/// assert_eq!(invoice.amount, Amount::from_minor(50_000));
/// assert_eq!(invoice.currency, Currency::SAR);
/// assert_eq!(invoice.description.as_deref(), Some("Annual subscription"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invoice {
    /// Unique UUID identifier for the invoice.
    pub id: Uuid,
    /// Current lifecycle status of the invoice.
    pub status: InvoiceStatus,
    /// Immutable invoice amount in the smallest currency unit (e.g., Halalas).
    pub amount: Amount,
    /// Immutable ISO 4217 currency code for the invoice.
    pub currency: Currency,
    /// Human-readable formatted amount string (e.g., `"500.00 SAR"`).
    #[serde(default)]
    pub amount_format: Option<String>,
    /// Optional merchant-supplied description shown on the hosted invoice.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional merchant logo URL rendered on the hosted payment page.
    #[serde(default)]
    pub logo_url: Option<String>,
    /// Hosted Moyasar payment page URL for customer checkout.
    #[serde(default)]
    pub url: Option<String>,
    /// Optional webhook or callback URL notified on invoice events.
    #[serde(default)]
    pub callback_url: Option<String>,
    /// Optional ISO 8601 expiration timestamp after which the invoice cannot be paid.
    #[serde(default)]
    pub expired_at: Option<String>,
    /// Optional URL to redirect the customer when navigating back from checkout.
    #[serde(default)]
    pub back_url: Option<String>,
    /// Optional URL to redirect the customer after successful payment.
    #[serde(default)]
    pub success_url: Option<String>,
    /// Optional validated key/value metadata attached to the invoice.
    #[serde(default)]
    pub metadata: Option<Metadata>,
    /// Associated payment records linked to this invoice.
    #[serde(default)]
    pub payments: Vec<serde_json::Value>,
    /// ISO 8601 creation timestamp.
    #[serde(default)]
    pub created_at: Option<String>,
    /// ISO 8601 last-update timestamp.
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invoice_status_deserializes_known_and_unknown_variants() {
        let cases = [
            (r#""created""#, InvoiceStatus::Created, "created"),
            (r#""paid""#, InvoiceStatus::Paid, "paid"),
            (r#""canceled""#, InvoiceStatus::Canceled, "canceled"),
            (r#""expired""#, InvoiceStatus::Expired, "expired"),
            (r#""unexpected_future""#, InvoiceStatus::Unknown, "unknown"),
        ];

        for (raw, expected, label) in cases {
            let parsed: InvoiceStatus = serde_json::from_str(raw).expect("valid status JSON");
            assert_eq!(parsed, expected);
            assert_eq!(parsed.as_str(), label);
            assert_eq!(parsed.to_string(), label);
        }
    }

    #[test]
    fn invoice_deserializes_full_payload_and_round_trips() {
        let raw = r#"{
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "status": "paid",
            "amount": 50000,
            "currency": "SAR",
            "amount_format": "500.00 SAR",
            "description": "Enterprise plan",
            "logo_url": "https://example.com/logo.png",
            "url": "https://moyasar.com/i/550e8400-e29b-41d4-a716-446655440000",
            "callback_url": "https://example.com/webhook",
            "expired_at": "2026-12-01T00:00:00Z",
            "back_url": "https://example.com/back",
            "success_url": "https://example.com/success",
            "metadata": { "order_id": "ORD-1001" },
            "payments": [{ "id": "pay_123", "status": "paid" }],
            "created_at": "2026-11-01T10:00:00Z",
            "updated_at": "2026-11-01T10:05:00Z"
        }"#;

        let invoice: Invoice = serde_json::from_str(raw).expect("valid full invoice");
        assert_eq!(
            invoice.id,
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap()
        );
        assert_eq!(invoice.status, InvoiceStatus::Paid);
        assert_eq!(invoice.amount, Amount::from_minor(50_000));
        assert_eq!(invoice.currency, Currency::SAR);
        assert_eq!(invoice.amount_format.as_deref(), Some("500.00 SAR"));
        assert_eq!(invoice.description.as_deref(), Some("Enterprise plan"));
        assert_eq!(
            invoice.logo_url.as_deref(),
            Some("https://example.com/logo.png")
        );
        assert_eq!(
            invoice.url.as_deref(),
            Some("https://moyasar.com/i/550e8400-e29b-41d4-a716-446655440000")
        );
        assert_eq!(
            invoice.callback_url.as_deref(),
            Some("https://example.com/webhook")
        );
        assert_eq!(invoice.expired_at.as_deref(), Some("2026-12-01T00:00:00Z"));
        assert_eq!(
            invoice.back_url.as_deref(),
            Some("https://example.com/back")
        );
        assert_eq!(
            invoice.success_url.as_deref(),
            Some("https://example.com/success")
        );
        assert_eq!(
            invoice.metadata.as_ref().and_then(|m| m.get("order_id")),
            Some("ORD-1001")
        );
        assert_eq!(invoice.payments.len(), 1);
        assert_eq!(invoice.created_at.as_deref(), Some("2026-11-01T10:00:00Z"));
        assert_eq!(invoice.updated_at.as_deref(), Some("2026-11-01T10:05:00Z"));

        let serialized = serde_json::to_string(&invoice).expect("serializes cleanly");
        let round_tripped: Invoice =
            serde_json::from_str(&serialized).expect("round-trips cleanly");
        assert_eq!(round_tripped, invoice);
    }

    #[test]
    fn invoice_deserializes_minimal_payload_with_defaults() {
        let raw = r#"{
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "status": "canceled",
            "amount": 1000,
            "currency": "KWD"
        }"#;

        let invoice: Invoice = serde_json::from_str(raw).expect("valid minimal invoice");
        assert_eq!(invoice.status, InvoiceStatus::Canceled);
        assert_eq!(invoice.amount, Amount::from_major(1, Currency::KWD));
        assert_eq!(invoice.currency, Currency::KWD);
        assert!(invoice.amount_format.is_none());
        assert!(invoice.description.is_none());
        assert!(invoice.logo_url.is_none());
        assert!(invoice.url.is_none());
        assert!(invoice.callback_url.is_none());
        assert!(invoice.expired_at.is_none());
        assert!(invoice.back_url.is_none());
        assert!(invoice.success_url.is_none());
        assert!(invoice.metadata.is_none());
        assert!(invoice.payments.is_empty());
        assert!(invoice.created_at.is_none());
        assert!(invoice.updated_at.is_none());
    }

    #[test]
    fn invoice_rejects_oversized_metadata_key() {
        let long_key = "k".repeat(41);
        let raw = format!(
            r#"{{
                "id": "550e8400-e29b-41d4-a716-446655440000",
                "status": "created",
                "amount": 500,
                "currency": "SAR",
                "metadata": {{ "{long_key}": "value" }}
            }}"#
        );

        let err = serde_json::from_str::<Invoice>(&raw).unwrap_err();
        assert!(err.to_string().contains("40 characters"));
    }
}
