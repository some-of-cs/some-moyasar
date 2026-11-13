//! Smart void-or-refund routing strategy with settlement and Mada window awareness (US-2.2.0 Unit 5).

use chrono::{DateTime, Duration, Utc};

use crate::kernel::auth::{AuthKey, SecretAuthKey};
use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::resources::payments::models::{Payment, PaymentStatus};
use crate::resources::payments::service::PaymentsService;
use crate::resources::payments::void::VOID_WINDOW_SECS;

/// Mada issuer authorization hold window in days before silent expiry.
pub const MADA_CAPTURE_WINDOW_DAYS: i64 = 14;

/// Reversal routing strategy selected for a payment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReversalStrategy {
    /// Issue `POST /v1/payments/:id/void` (unsettled / authorized payments).
    Void,
    /// Issue `POST /v1/payments/:id/refund` with an optional partial `amount`.
    Refund {
        /// Optional partial refund amount in minor units (`None` = full refund).
        amount: Option<u64>,
    },
    /// Automatically evaluate the settlement window and route to `Void` or `Refund`.
    Auto,
}

/// Alias for [`ReversalStrategy`] matching the US-2.2.0 domain vocabulary.
pub type VoidOrRefund = ReversalStrategy;

/// Pure planning outcome combining the chosen [`ReversalStrategy`] with Mada hold-expiry status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReversalPlan {
    /// Selected reversal operation (`Void` or `Refund`).
    pub strategy: ReversalStrategy,
    /// `true` when an `Authorized` payment is older than [`MADA_CAPTURE_WINDOW_DAYS`] (14 days).
    pub mada_hold_likely_expired: bool,
}

impl<K: AuthKey> PaymentsService<'_, K> {
    /// Pure, network-free reversal planner (`CC <= 6`, `nesting <= 2`, zero clones).
    ///
    /// - `Authorized` -> [`ReversalStrategy::Void`]
    /// - `Paid | Captured` within [`VOID_WINDOW_SECS`] (2 hours) -> [`ReversalStrategy::Void`]
    /// - Otherwise (settled) -> [`ReversalStrategy::Refund`] with `amount: None`
    pub fn plan_reversal(&self, payment: &Payment, now: DateTime<Utc>) -> ReversalStrategy {
        let age = now - payment.created_at;
        let unsettled = age.num_seconds() < VOID_WINDOW_SECS;
        match (&payment.status, unsettled) {
            (PaymentStatus::Authorized, _) => ReversalStrategy::Void,
            (PaymentStatus::Paid | PaymentStatus::Captured, true) => ReversalStrategy::Void,
            _ => ReversalStrategy::Refund { amount: None },
        }
    }

    /// Returns `true` when an `Authorized` payment has exceeded the 14-day Mada capture window.
    pub fn is_mada_hold_expired(&self, payment: &Payment, now: DateTime<Utc>) -> bool {
        matches!(payment.status, PaymentStatus::Authorized)
            && (now - payment.created_at) > Duration::days(MADA_CAPTURE_WINDOW_DAYS)
    }

    /// Produces a full [`ReversalPlan`] including Mada 14-day hold-expiry awareness.
    pub fn build_reversal_plan(&self, payment: &Payment, now: DateTime<Utc>) -> ReversalPlan {
        ReversalPlan {
            strategy: self.plan_reversal(payment, now),
            mada_hold_likely_expired: self.is_mada_hold_expired(payment, now),
        }
    }
}

impl<K: SecretAuthKey> PaymentsService<'_, K> {
    /// Executes smart void-or-refund routing for `payment` at timestamp `now`.
    ///
    /// Never attempts `void()` once the 2-hour unsettled window has elapsed on a
    /// `Paid` or `Captured` payment. If an in-window `void()` is rejected by the API
    /// as non-cancelable, automatically falls back to a full `refund()`.
    pub async fn reverse(
        &self,
        payment: &Payment,
        now: DateTime<Utc>,
    ) -> Result<Payment, MoyasarError> {
        let id = payment.id.to_string();
        match self.plan_reversal(payment, now) {
            ReversalStrategy::Void => match self.void(&id).await {
                Ok(voided) => Ok(voided),
                Err(MoyasarError::Api {
                    error_type: ApiErrorType::InvalidRequestError | ApiErrorType::IntentNotCancelable,
                    ..
                }) if matches!(payment.status, PaymentStatus::Paid | PaymentStatus::Captured) => {
                    self.refund(&id, None).await
                }
                Err(err) => Err(err),
            },
            ReversalStrategy::Refund { amount } => self.refund(&id, amount).await,
            ReversalStrategy::Auto => self.refund(&id, None).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn void_or_refund_alias_matches_reversal_strategy() {
        let strategy: VoidOrRefund = ReversalStrategy::Auto;
        assert_eq!(strategy, ReversalStrategy::Auto);
    }

    #[test]
    fn mada_capture_window_constant_is_fourteen_days() {
        assert_eq!(MADA_CAPTURE_WINDOW_DAYS, 14);
        assert_eq!(VOID_WINDOW_SECS, 7_200);
    }
}
