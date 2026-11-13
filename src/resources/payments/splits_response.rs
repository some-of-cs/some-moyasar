//! Marketplace split response models and proportional refund calculator (US-2.5.0 Unit 4).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::resources::payments::splits::SplitRecipientType;

fn default_refundable_true() -> bool {
    true
}

/// Split allocation entry returned by the Moyasar Payments API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentSplitResponse {
    /// Optional server-assigned split identifier.
    #[serde(default)]
    pub id: Option<Uuid>,
    /// Split amount in the smallest currency unit.
    pub amount: u64,
    /// Role of the recipient receiving this split.
    pub recipient_type: SplitRecipientType,
    /// Moyasar UUID of the recipient account.
    pub recipient_id: Uuid,
    /// Reconciliation reference persisted into settlement files.
    #[serde(default)]
    pub reference: Option<String>,
    /// Optional human-readable description.
    #[serde(default)]
    pub description: Option<String>,
    /// Whether this split absorbs Moyasar processing fees.
    #[serde(default)]
    pub fee_source: Option<bool>,
    /// Whether this split reverses proportionally on parent payment refund.
    #[serde(default = "default_refundable_true")]
    pub refundable: bool,
    /// Current settlement status of the split (e.g., `"pending"`, `"settled"`).
    #[serde(default)]
    pub status: Option<String>,
}

/// Calculated proportional refund allocation for a single refundable split.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitRefundAllocation {
    /// Recipient UUID for the reversed split.
    pub recipient_id: Uuid,
    /// Reconciliation reference of the split, if present.
    pub reference: Option<String>,
    /// Proportional refund amount in minor currency units.
    pub refund_amount: u64,
}

/// Calculates proportional refund allocations across refundable splits.
///
/// Non-refundable splits (`refundable == false`) are excluded so they are preserved
/// during parent payment refunds. Returns an empty vector if `payment_total == 0`
/// or `refund_amount == 0`.
pub fn calculate_proportional_refunds(
    splits: &[PaymentSplitResponse],
    payment_total: u64,
    refund_amount: u64,
) -> Vec<SplitRefundAllocation> {
    if payment_total == 0 || refund_amount == 0 {
        return Vec::new();
    }

    let capped_refund = refund_amount.min(payment_total) as u128;
    let total = payment_total as u128;

    splits
        .iter()
        .filter(|split| split.refundable)
        .map(|split| {
            let proportional = (split.amount as u128 * capped_refund) / total;
            SplitRefundAllocation {
                recipient_id: split.recipient_id,
                reference: split.reference.clone(),
                refund_amount: proportional as u64,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deserializes_split_response_with_default_refundable() {
        let raw = json!({
            "amount": 13500,
            "recipient_type": "beneficiary",
            "recipient_id": Uuid::nil(),
            "reference": "vendor-101"
        });
        let resp: PaymentSplitResponse =
            serde_json::from_value(raw).expect("PaymentSplitResponse must deserialize");
        assert!(resp.refundable);
        assert_eq!(resp.amount, 13_500);
        assert_eq!(resp.recipient_type, SplitRecipientType::Beneficiary);
    }

    #[test]
    fn proportional_refund_cascade_skips_non_refundable_splits() {
        let vendor_id = Uuid::nil();
        let platform_id = Uuid::max();

        let splits = vec![
            PaymentSplitResponse {
                id: None,
                amount: 13_500,
                recipient_type: SplitRecipientType::Beneficiary,
                recipient_id: vendor_id,
                reference: Some("vendor-1".into()),
                description: None,
                fee_source: Some(false),
                refundable: true,
                status: None,
            },
            PaymentSplitResponse {
                id: None,
                amount: 1_500,
                recipient_type: SplitRecipientType::Platform,
                recipient_id: platform_id,
                reference: Some("platform-fee".into()),
                description: None,
                fee_source: Some(true),
                refundable: false,
                status: None,
            },
        ];

        // 50% refund of 15_000 -> 7_500; only refundable vendor split reverses proportionally (6_750).
        let allocations = calculate_proportional_refunds(&splits, 15_000, 7_500);
        assert_eq!(allocations.len(), 1);
        assert_eq!(allocations[0].recipient_id, vendor_id);
        assert_eq!(allocations[0].refund_amount, 6_750);
    }
}
