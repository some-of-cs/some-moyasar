//! Subscription domain models and `RefundStrategy`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::kernel::types::Currency;
use crate::resources::tokens::TokenId;

/// Unique identifier for a single subscription billing cycle.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubscriptionCycleId(pub String);

impl SubscriptionCycleId {
    /// Creates a new [`SubscriptionCycleId`] from a subscription ID and cycle index.
    #[must_use]
    pub fn new(subscription_id: &str, cycle_index: u64) -> Self {
        Self(format!("{subscription_id}:{cycle_index}"))
    }

    /// Returns the underlying string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Strategy for reversing a subscription cycle payment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefundStrategy {
    /// Reverse the entire cycle payment (uses `/void` if unsettled within 2 hours).
    Full,
    /// Refund an explicit partial amount in minor units.
    Partial {
        /// Partial refund amount in minor units (e.g., halalas).
        amount_halalas: u64,
    },
    /// Refund the exact unused portion of the billing period using integer ceiling math.
    ProRata {
        /// UTC timestamp when the subscription was canceled.
        canceled_at: DateTime<Utc>,
    },
}

/// Parameters for refunding or voiding a subscription billing cycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionCycleRefundParams {
    /// Merchant subscription identifier.
    pub subscription_id: String,
    /// Moyasar payment ID associated with the cycle charge.
    pub cycle_payment_id: String,
    /// Start of the billing period (UTC).
    pub period_start: DateTime<Utc>,
    /// End of the billing period (UTC).
    pub period_end: DateTime<Utc>,
    /// Reversal strategy (`Full`, `Partial`, or `ProRata`).
    pub strategy: RefundStrategy,
}

/// Action executed by Moyasar for a subscription cycle reversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReversalAction {
    /// Full reversal executed via `POST /v1/payments/:id/void`.
    Voided,
    /// Reversal executed via `POST /v1/payments/:id/refund`.
    Refunded,
}

/// Result of a subscription cycle refund or void operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionCycleRefundResult {
    /// Underlying Moyasar payment identifier.
    pub payment_id: String,
    /// Whether the cycle was voided or refunded.
    pub action: ReversalAction,
    /// Minor-unit amount reversed.
    pub amount_reversed: u64,
}

/// Record of a single subscription billing cycle attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionCycle {
    /// Deterministic cycle identifier (`"{sub_id}:{cycle_index}"`).
    pub id: SubscriptionCycleId,
    /// Zero-based or monotonic billing cycle index.
    pub cycle_index: u64,
    /// Cycle charge amount in minor units.
    pub amount: u64,
    /// ISO 4217 currency.
    pub currency: Currency,
    /// Moyasar payment ID once charged.
    pub payment_id: Option<String>,
    /// Billing period start (UTC).
    pub period_start: DateTime<Utc>,
    /// Billing period end (UTC).
    pub period_end: DateTime<Utc>,
}

/// Merchant subscription aggregate referencing a saved [`TokenId`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    /// Merchant subscription identifier.
    pub id: String,
    /// Merchant customer identifier.
    pub customer_id: String,
    /// Active saved card token (`token_...`).
    pub token_id: TokenId,
    /// Current billing cycle index.
    pub current_cycle: u64,
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};

    use super::*;

    #[test]
    fn subscription_models_and_refund_strategy_roundtrip_serde() {
        let cycle_id = SubscriptionCycleId::new("sub_pro_99", 4);
        assert_eq!(cycle_id.as_str(), "sub_pro_99:4");

        let sub = Subscription {
            id: "sub_pro_99".to_owned(),
            customer_id: "cus_101".to_owned(),
            token_id: TokenId::new("token_abc123").unwrap(),
            current_cycle: 4,
        };
        let sub_json = serde_json::to_string(&sub).unwrap();
        let sub_back: Subscription = serde_json::from_str(&sub_json).unwrap();
        assert_eq!(sub_back, sub);

        let start = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
        let end = start + Duration::days(30);
        let cycle = SubscriptionCycle {
            id: cycle_id,
            cycle_index: 4,
            amount: 29_900,
            currency: Currency::SAR,
            payment_id: Some("pay_sub_4".to_owned()),
            period_start: start,
            period_end: end,
        };
        let cycle_json = serde_json::to_string(&cycle).unwrap();
        let cycle_back: SubscriptionCycle = serde_json::from_str(&cycle_json).unwrap();
        assert_eq!(cycle_back, cycle);

        for strategy in [
            RefundStrategy::Full,
            RefundStrategy::Partial {
                amount_halalas: 5000,
            },
            RefundStrategy::ProRata {
                canceled_at: start + Duration::days(10),
            },
        ] {
            let encoded = serde_json::to_string(&strategy).unwrap();
            let decoded: RefundStrategy = serde_json::from_str(&encoded).unwrap();
            assert_eq!(decoded, strategy);
        }
    }
}
