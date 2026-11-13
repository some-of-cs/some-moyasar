//! Smart void-or-refund routing strategy with settlement and Mada window awareness (US-2.2.0 Unit 5).

use chrono::{DateTime, Duration, Utc};

use crate::kernel::auth::{AuthKey, SecretAuthKey};
use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::resources::payments::models::{Payment, PaymentStatus};
use crate::resources::payments::service::PaymentsService;
use crate::resources::payments::void::VOID_WINDOW_SECS;

pub use crate::resources::payments::capture::MADA_CAPTURE_WINDOW_DAYS;

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

fn is_void_fallback_eligible(payment: &Payment, err: &MoyasarError) -> bool {
    matches!(
        payment.status,
        PaymentStatus::Paid | PaymentStatus::Captured
    ) && matches!(
        err,
        MoyasarError::Api {
            error_type: ApiErrorType::InvalidRequestError | ApiErrorType::IntentNotCancelable,
            ..
        }
    )
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
    ///
    /// # Examples
    ///
    /// Publishable keys cannot invoke `reverse`:
    ///
    /// ```compile_fail
    /// use chrono::Utc;
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    ///
    /// # async fn example(payment: some_moyasar::resources::payments::models::Payment) {
    /// let pk = PublishableKey::new("pk_test_abc123").unwrap();
    /// let client = MoyasarClient::new(pk).unwrap();
    /// let _ = client.payments().reverse(&payment, Utc::now()).await;
    /// # }
    /// ```
    pub async fn reverse(
        &self,
        payment: &Payment,
        now: DateTime<Utc>,
    ) -> Result<Payment, MoyasarError> {
        let id = payment.id.as_str();
        match self.plan_reversal(payment, now) {
            ReversalStrategy::Void => match self.void(id).await {
                Ok(voided) => Ok(voided),
                Err(err) if is_void_fallback_eligible(payment, &err) => self.refund(id, None).await,
                Err(err) => Err(err),
            },
            ReversalStrategy::Refund { amount } => self.refund(id, amount).await,
            ReversalStrategy::Auto => self.refund(id, None).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::client::MoyasarClient;
    use crate::kernel::metadata::Metadata;
    use crate::kernel::types::Currency;
    use serde_json::json;

    fn sample_payment(id: &str, status: PaymentStatus, created_at: DateTime<Utc>) -> Payment {
        Payment {
            id: id.to_owned(),
            status,
            amount: 15000,
            amount_format: "150.00 SAR".to_owned(),
            fee: 350,
            fee_format: "3.50 SAR".to_owned(),
            refunded: 0,
            refunded_format: "0.00 SAR".to_owned(),
            captured: 15000,
            captured_format: "150.00 SAR".to_owned(),
            currency: Currency::SAR,
            description: None,
            source: json!({ "type": "creditcard" }),
            metadata: Metadata::default(),
            splits: None,
            callback_url: None,
            invoice_id: None,
            ip: None,
            captured_at: None,
            refunded_at: None,
            voided_at: None,
            created_at,
            updated_at: created_at,
        }
    }

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

    #[test]
    fn tc_2_2_0_06_plan_reversal_routes_by_status_and_two_hour_window() {
        let key = SecretKey::new("sk_test_smart_void_plan").expect("valid key");
        let client = MoyasarClient::new(key).expect("client builds");
        let svc = client.payments();
        let now = Utc::now();
        let within = now - Duration::minutes(45);
        let at_boundary = now - Duration::seconds(VOID_WINDOW_SECS);
        let settled = now - Duration::hours(5);

        assert_eq!(
            svc.plan_reversal(
                &sample_payment("pay_auth_new", PaymentStatus::Authorized, within),
                now
            ),
            ReversalStrategy::Void
        );
        assert_eq!(
            svc.plan_reversal(
                &sample_payment("pay_auth_old", PaymentStatus::Authorized, settled),
                now
            ),
            ReversalStrategy::Void
        );
        assert_eq!(
            svc.plan_reversal(
                &sample_payment("pay_paid_new", PaymentStatus::Paid, within),
                now
            ),
            ReversalStrategy::Void
        );
        assert_eq!(
            svc.plan_reversal(
                &sample_payment("pay_cap_new", PaymentStatus::Captured, within),
                now
            ),
            ReversalStrategy::Void
        );
        assert_eq!(
            svc.plan_reversal(
                &sample_payment("pay_paid_edge", PaymentStatus::Paid, at_boundary),
                now
            ),
            ReversalStrategy::Refund { amount: None }
        );
        assert_eq!(
            svc.plan_reversal(
                &sample_payment("pay_paid_old", PaymentStatus::Paid, settled),
                now
            ),
            ReversalStrategy::Refund { amount: None }
        );
        assert_eq!(
            svc.plan_reversal(
                &sample_payment("pay_cap_old", PaymentStatus::Captured, settled),
                now
            ),
            ReversalStrategy::Refund { amount: None }
        );
        assert_eq!(
            svc.plan_reversal(
                &sample_payment("pay_voided", PaymentStatus::Voided, within),
                now
            ),
            ReversalStrategy::Refund { amount: None }
        );
    }

    #[test]
    fn tc_2_2_0_07_is_mada_hold_expired_and_build_reversal_plan() {
        let key = SecretKey::new("sk_test_mada_window").expect("valid key");
        let client = MoyasarClient::new(key).expect("client builds");
        let svc = client.payments();
        let now = Utc::now();
        let day_14 = now - Duration::days(MADA_CAPTURE_WINDOW_DAYS);
        let day_15 = now - Duration::days(MADA_CAPTURE_WINDOW_DAYS + 1);

        let auth_14 = sample_payment("pay_mada_14", PaymentStatus::Authorized, day_14);
        assert!(!svc.is_mada_hold_expired(&auth_14, now));
        assert_eq!(
            svc.build_reversal_plan(&auth_14, now),
            ReversalPlan {
                strategy: ReversalStrategy::Void,
                mada_hold_likely_expired: false,
            }
        );

        let auth_15 = sample_payment("pay_mada_15", PaymentStatus::Authorized, day_15);
        assert!(svc.is_mada_hold_expired(&auth_15, now));
        assert_eq!(
            svc.build_reversal_plan(&auth_15, now),
            ReversalPlan {
                strategy: ReversalStrategy::Void,
                mada_hold_likely_expired: true,
            }
        );

        let paid_15 = sample_payment("pay_paid_15", PaymentStatus::Paid, day_15);
        assert!(!svc.is_mada_hold_expired(&paid_15, now));
        assert_eq!(
            svc.build_reversal_plan(&paid_15, now),
            ReversalPlan {
                strategy: ReversalStrategy::Refund { amount: None },
                mada_hold_likely_expired: false,
            }
        );
    }

    #[tokio::test]
    async fn tc_2_2_0_06_reverse_rejects_unsafe_payment_id_before_network() {
        let key = SecretKey::new("sk_test_reverse_id").expect("valid key");
        let client = MoyasarClient::new(key).expect("client builds");
        let now = Utc::now();
        let bad_payment = sample_payment(
            "pay_123/refund",
            PaymentStatus::Paid,
            now - Duration::hours(5),
        );

        let err = client
            .payments()
            .reverse(&bad_payment, now)
            .await
            .expect_err("must reject unsafe payment id");
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            }
        ));
    }
}
