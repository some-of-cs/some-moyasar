//! Virtual Stripe-compatible `RefundsService` (`client.refunds()`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::kernel::auth::{AuthKey, SecretAuthKey};
use crate::kernel::client::MoyasarClient;
use crate::kernel::error::MoyasarError;
use crate::resources::payments::{Payment, PaymentId, PaymentStatus, can_void_payment};

/// Parameters for creating a virtual Stripe-style refund receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefundCreateParams {
    /// Target Moyasar payment identifier.
    pub payment_id: String,
    /// Optional partial refund amount in minor units; `None` reverses the full amount.
    pub amount: Option<u64>,
    /// When `true` and reversing the full amount within ~2 hours, prefer `/void` over `/refund`.
    pub prefer_void_if_unsettled: bool,
}

impl RefundCreateParams {
    /// Creates refund parameters for `payment_id` with full reversal and smart void routing enabled.
    #[must_use]
    pub fn new(payment_id: impl Into<String>) -> Self {
        Self {
            payment_id: payment_id.into(),
            amount: None,
            prefer_void_if_unsettled: true,
        }
    }

    /// Sets an explicit partial refund amount in minor units.
    #[must_use]
    pub const fn amount(mut self, amount: u64) -> Self {
        self.amount = Some(amount);
        self
    }

    /// Configures whether full reversals within the unsettled window prefer `/void`.
    #[must_use]
    pub const fn prefer_void_if_unsettled(mut self, prefer: bool) -> Self {
        self.prefer_void_if_unsettled = prefer;
        self
    }
}

/// Synthesized Stripe-style refund receipt backed by a Moyasar [`Payment`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefundReceipt {
    /// Underlying Moyasar payment identifier (`SAMA` audit link).
    pub payment_id: String,
    /// Resulting payment status (`refunded`, `voided`, etc.).
    pub status: PaymentStatus,
    /// Total refunded minor-unit amount (or reversed amount when voided).
    pub refunded_amount: u64,
    /// Formatted refunded amount string from Moyasar.
    pub refunded_format: String,
    /// Timestamp when the payment was refunded, if applicable.
    pub refunded_at: Option<DateTime<Utc>>,
    /// Timestamp when the payment was voided, if applicable.
    pub voided_at: Option<DateTime<Utc>>,
    /// Per-split reversal indicators extracted from `payment.splits`.
    pub split_reversal_flags: Vec<bool>,
}

impl RefundReceipt {
    /// Synthesizes a [`RefundReceipt`] from a Moyasar [`Payment`].
    #[must_use]
    pub fn from_payment(payment: &Payment) -> Self {
        let split_reversal_flags = payment
            .splits
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|split| {
                split
                    .get("refunded")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
            })
            .collect();
        let refunded_amount = if payment.status == PaymentStatus::Voided && payment.refunded == 0 {
            payment.amount
        } else {
            payment.refunded
        };
        Self {
            payment_id: payment.id.clone(),
            status: payment.status.clone(),
            refunded_amount,
            refunded_format: payment.refunded_format.clone(),
            refunded_at: payment.refunded_at,
            voided_at: payment.voided_at,
            split_reversal_flags,
        }
    }
}

/// Virtual Stripe-compatible refunds service (`client.refunds()`).
///
/// # Security
///
/// Reversal operations require a [`SecretKey`](crate::kernel::auth::SecretKey)
/// and fail to compile on `MoyasarClient<PublishableKey>`:
///
/// ```compile_fail
/// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
/// use some_moyasar::stripe_compat::refunds::RefundCreateParams;
///
/// # async fn demo() {
/// let key = PublishableKey::new("pk_test_123").unwrap();
/// let client = MoyasarClient::new(key).unwrap();
/// let _ = client.refunds().create(&RefundCreateParams::new("pay_123")).await;
/// # }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct RefundsService<'a, K: AuthKey> {
    client: &'a MoyasarClient<K>,
}

impl<'a, K: AuthKey> RefundsService<'a, K> {
    /// Returns a reference to the underlying [`MoyasarClient`].
    #[must_use]
    pub const fn client(&self) -> &'a MoyasarClient<K> {
        self.client
    }
}

impl<K: AuthKey> MoyasarClient<K> {
    /// Returns a [`RefundsService`] handle for virtual Stripe-style refunds.
    #[must_use]
    pub const fn refunds(&self) -> RefundsService<'_, K> {
        RefundsService { client: self }
    }
}

impl<K: SecretAuthKey> RefundsService<'_, K> {
    /// Executes a refund or smart unsettled void and returns a [`RefundReceipt`].
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError`] on invalid payment IDs, HTTP/API errors, or
    /// invalid refund amounts.
    pub async fn create(&self, params: &RefundCreateParams) -> Result<RefundReceipt, MoyasarError> {
        let id = PaymentId::new(&params.payment_id)?;
        let payments = self.client.payments();
        let payment = payments.retrieve(&id).await?;
        let reversed = if should_void(&payment, params) {
            payments.void(id.as_str()).await?
        } else {
            payments.refund(id.as_str(), params.amount).await?
        };
        Ok(RefundReceipt::from_payment(&reversed))
    }
}

fn should_void(payment: &Payment, params: &RefundCreateParams) -> bool {
    let is_partial = params.amount.is_some_and(|amt| amt < payment.amount);
    params.prefer_void_if_unsettled && !is_partial && can_void_payment(payment, Utc::now())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;
    use crate::kernel::types::Currency;

    #[test]
    fn synthesizes_refund_receipt_and_split_flags() {
        let now = Utc.with_ymd_and_hms(2026, 11, 14, 10, 0, 0).unwrap();
        let payment = Payment {
            id: "pay_01J9TEST".to_owned(),
            status: PaymentStatus::Refunded,
            amount: 15000,
            amount_format: "150.00 SAR".to_owned(),
            fee: 0,
            fee_format: "0.00 SAR".to_owned(),
            refunded: 5000,
            refunded_format: "50.00 SAR".to_owned(),
            captured: 15000,
            captured_format: "150.00 SAR".to_owned(),
            currency: Currency::SAR,
            description: None,
            source: json!({"type": "creditcard"}),
            metadata: Default::default(),
            splits: Some(vec![json!({"refunded": true}), json!({"refunded": false})]),
            callback_url: None,
            invoice_id: None,
            ip: None,
            captured_at: Some(now),
            refunded_at: Some(now),
            voided_at: None,
            created_at: now,
            updated_at: now,
        };
        let receipt = RefundReceipt::from_payment(&payment);
        assert_eq!(receipt.payment_id, "pay_01J9TEST");
        assert_eq!(receipt.refunded_amount, 5000);
        assert_eq!(receipt.split_reversal_flags, vec![true, false]);
    }
}
