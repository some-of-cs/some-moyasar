//! Marketplace payment split request types (US-2.5.0 Unit 1).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn default_refundable() -> Option<bool> {
    Some(true)
}

/// Recipient role for a marketplace payment split.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitRecipientType {
    /// Registered Moyasar entity account.
    Entity,
    /// Marketplace platform commission account.
    Platform,
    /// Vendor or sub-merchant beneficiary account.
    Beneficiary,
}

/// A single split instruction attached to a marketplace payment creation request.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::payments::splits::{PaymentSplitRequest, SplitRecipientType};
/// use uuid::Uuid;
///
/// let split = PaymentSplitRequest::new(
///     13_500,
///     SplitRecipientType::Beneficiary,
///     Uuid::nil(),
/// )
/// .with_reference("vendor-order-1001")
/// .with_fee_source(false);
///
/// assert!(split.is_refundable());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentSplitRequest {
    /// Split amount in the smallest currency unit (e.g., Halalas).
    pub amount: u64,
    /// Role of the recipient receiving this split.
    pub recipient_type: SplitRecipientType,
    /// Moyasar UUID of the recipient account.
    pub recipient_id: Uuid,
    /// Stable, unique reconciliation reference persisted into settlement files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// Optional human-readable description for the split.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Designates whether this split absorbs Moyasar processing fees.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_source: Option<bool>,
    /// Whether this split reverses proportionally on parent payment refund (defaults to `true`).
    #[serde(default = "default_refundable", skip_serializing_if = "Option::is_none")]
    pub refundable: Option<bool>,
}

impl PaymentSplitRequest {
    /// Constructs a new [`PaymentSplitRequest`] with `refundable` defaulting to `true`.
    pub const fn new(
        amount: u64,
        recipient_type: SplitRecipientType,
        recipient_id: Uuid,
    ) -> Self {
        Self {
            amount,
            recipient_type,
            recipient_id,
            reference: None,
            description: None,
            fee_source: None,
            refundable: Some(true),
        }
    }

    /// Attaches a reconciliation `reference` to this split.
    pub fn with_reference(mut self, reference: impl Into<String>) -> Self {
        self.reference = Some(reference.into());
        self
    }

    /// Attaches a `description` to this split.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets `fee_source` to designate whether this split absorbs processing fees.
    pub const fn with_fee_source(mut self, fee_source: bool) -> Self {
        self.fee_source = Some(fee_source);
        self
    }

    /// Sets `refundable` to control proportional reversal on parent payment refund.
    pub const fn with_refundable(mut self, refundable: bool) -> Self {
        self.refundable = Some(refundable);
        self
    }

    /// Returns `true` if this split is refundable (defaults to `true` when omitted).
    pub const fn is_refundable(&self) -> bool {
        match self.refundable {
            Some(flag) => flag,
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tc_2_5_0_01_two_way_split_serialization_with_typed_roles() {
        let vendor_id = Uuid::nil();
        let platform_id = Uuid::max();

        let splits = vec![
            PaymentSplitRequest::new(13_500, SplitRecipientType::Beneficiary, vendor_id)
                .with_reference("vendor-1")
                .with_fee_source(false),
            PaymentSplitRequest::new(1_500, SplitRecipientType::Platform, platform_id)
                .with_reference("platform-fee")
                .with_fee_source(true),
        ];

        let serialized = serde_json::to_value(&splits).expect("splits must serialize");
        let arr = serialized.as_array().expect("splits must be an array");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["recipient_type"], json!("beneficiary"));
        assert_eq!(arr[0]["amount"], json!(13_500));
        assert_eq!(arr[0]["fee_source"], json!(false));
        assert_eq!(arr[1]["recipient_type"], json!("platform"));
        assert_eq!(arr[1]["amount"], json!(1_500));
        assert_eq!(arr[1]["fee_source"], json!(true));
    }

    #[test]
    fn tc_2_5_0_05_refundable_defaults_to_true_when_omitted() {
        let raw = json!({
            "amount": 5000,
            "recipient_type": "entity",
            "recipient_id": Uuid::nil()
        });
        let parsed: PaymentSplitRequest =
            serde_json::from_value(raw).expect("split without refundable must deserialize");
        assert_eq!(parsed.refundable, Some(true));
        assert!(parsed.is_refundable());

        let omitted = PaymentSplitRequest {
            amount: 5000,
            recipient_type: SplitRecipientType::Entity,
            recipient_id: Uuid::nil(),
            reference: None,
            description: None,
            fee_source: None,
            refundable: None,
        };
        assert!(omitted.is_refundable());
        let val = serde_json::to_value(&omitted).expect("must serialize");
        assert!(val.get("refundable").is_none());
    }
}
