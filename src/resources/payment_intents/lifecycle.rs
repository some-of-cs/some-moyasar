//! Payment Intent fulfillment lifecycle outcome mapping (`FulfillmentOutcome`).
//!
//! # Fulfilled != Paid Law
//!
//! When a `PaymentIntent` transitions to `PaymentIntentStatus::Fulfilled`, that
//! only means a fulfillment attempt was submitted — **not** that funds were captured.
//! Always classify the returned [`IntentPayment`] via [`FulfillmentOutcome::from_payment`]
//! and re-verify final settlement on the backend via `GET /v1/payments/:id`.

use crate::resources::payment_intents::fulfill::{IntentPayment, IntentPaymentStatus};

/// Strongly-typed outcome derived from an [`IntentPayment`] returned by `fulfill()`.
#[derive(Debug, Clone)]
pub enum FulfillmentOutcome {
    /// Payment settled (`status == Paid` or `Captured`).
    Paid(IntentPayment),
    /// Payment authorized (`status == Authorized`, manual capture required).
    Authorized(IntentPayment),
    /// Payment requires customer action (e.g., 3DS challenge at `transaction_url`).
    RequiresAction {
        /// Underlying initiated payment.
        payment: IntentPayment,
        /// Optional 3DS or redirect URL extracted from `payment.source["transaction_url"]`.
        transaction_url: Option<String>,
    },
    /// Payment attempt failed or was voided/refunded.
    Failed(IntentPayment),
}

impl FulfillmentOutcome {
    /// Classifies an [`IntentPayment`] into a [`FulfillmentOutcome`].
    ///
    /// # Examples
    ///
    /// ```
    /// use some_moyasar::resources::payment_intents::fulfill::{IntentPayment, IntentPaymentStatus};
    /// use some_moyasar::resources::payment_intents::lifecycle::FulfillmentOutcome;
    ///
    /// let payment: IntentPayment = serde_json::from_str(r#"{
    ///     "id": "pay_1",
    ///     "status": "initiated",
    ///     "amount": 25000,
    ///     "currency": "SAR",
    ///     "source": { "transaction_url": "https://api.moyasar.com/3ds" },
    ///     "created_at": "2026-11-13T11:00:00Z"
    /// }"#).unwrap();
    /// let outcome = FulfillmentOutcome::from_payment(payment);
    /// assert!(!outcome.is_settled());
    /// assert_eq!(outcome.payment().status, IntentPaymentStatus::Initiated);
    /// ```
    pub fn from_payment(payment: IntentPayment) -> Self {
        match payment.status {
            IntentPaymentStatus::Paid | IntentPaymentStatus::Captured => Self::Paid(payment),
            IntentPaymentStatus::Authorized => Self::Authorized(payment),
            IntentPaymentStatus::Initiated | IntentPaymentStatus::Verified => {
                let transaction_url = extract_transaction_url(&payment);
                Self::RequiresAction {
                    payment,
                    transaction_url,
                }
            }
            IntentPaymentStatus::Failed
            | IntentPaymentStatus::Refunded
            | IntentPaymentStatus::Voided
            | IntentPaymentStatus::Unknown => Self::Failed(payment),
        }
    }

    /// Returns `true` only when the underlying payment is settled (`Paid` / `Captured`).
    pub const fn is_settled(&self) -> bool {
        matches!(self, Self::Paid(_))
    }

    /// Returns a shared reference to the underlying [`IntentPayment`] regardless of outcome variant.
    pub const fn payment(&self) -> &IntentPayment {
        match self {
            Self::Paid(p) | Self::Authorized(p) | Self::Failed(p) => p,
            Self::RequiresAction { payment, .. } => payment,
        }
    }
}

fn extract_transaction_url(payment: &IntentPayment) -> Option<String> {
    payment
        .source
        .as_ref()?
        .get("transaction_url")?
        .as_str()
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payment_with_status(status: &str, tx_url: Option<&str>) -> IntentPayment {
        let source = match tx_url {
            Some(url) => format!(r#"{{"type":"creditcard","transaction_url":"{url}"}}"#),
            None => "null".to_owned(),
        };
        let raw = format!(
            r#"{{
                "id": "pay_test_lc",
                "status": "{status}",
                "amount": 15000,
                "currency": "SAR",
                "source": {source},
                "created_at": "2026-11-13T11:00:00Z"
            }}"#
        );
        serde_json::from_str(&raw).expect("valid payment")
    }

    #[test]
    fn tc_3_2_0_05_fulfilled_does_not_imply_paid() {
        let initiated = payment_with_status("initiated", Some("https://moyasar.com/3ds/1"));
        let outcome = FulfillmentOutcome::from_payment(initiated);
        assert!(!outcome.is_settled());
        assert_eq!(outcome.payment().id, "pay_test_lc");
        assert!(matches!(
            outcome,
            FulfillmentOutcome::RequiresAction {
                transaction_url: Some(_),
                ..
            }
        ));

        let failed = payment_with_status("failed", None);
        let failed_outcome = FulfillmentOutcome::from_payment(failed);
        assert!(!failed_outcome.is_settled());
        assert!(matches!(failed_outcome, FulfillmentOutcome::Failed(_)));
    }

    #[test]
    fn classifies_paid_and_authorized_outcomes() {
        let paid = FulfillmentOutcome::from_payment(payment_with_status("paid", None));
        assert!(paid.is_settled());

        let auth = FulfillmentOutcome::from_payment(payment_with_status("authorized", None));
        assert!(!auth.is_settled());
        assert!(matches!(auth, FulfillmentOutcome::Authorized(_)));
    }
}
