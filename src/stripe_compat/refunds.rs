//! Virtual Stripe-compatible `RefundsService` (`client.refunds()`).

use std::borrow::Borrow;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::kernel::auth::{AuthKey, SecretAuthKey};
use crate::kernel::client::MoyasarClient;
use crate::kernel::error::MoyasarError;
use crate::kernel::types::Currency;
use crate::resources::payments::{Payment, is_void_fallback_eligible, PaymentId, PaymentStatus, validate_refund_amount};
use crate::stripe_compat::refund_strategy::{RefundRoute, select_refund_route};

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
    /// ISO 4217 currency of the reversed payment.
    pub currency: Currency,
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
        let split_reversal_flags = extract_split_reversal_flags(payment);
        let is_voided_full = payment.status == PaymentStatus::Voided && payment.refunded == 0;
        let (refunded_amount, refunded_format) = if is_voided_full {
            (payment.amount, payment.amount_format.clone())
        } else {
            (payment.refunded, payment.refunded_format.clone())
        };
        Self {
            payment_id: payment.id.clone(),
            status: payment.status.clone(),
            currency: payment.currency,
            refunded_amount,
            refunded_format,
            refunded_at: payment.refunded_at,
            voided_at: payment.voided_at,
            split_reversal_flags,
        }
    }
}

fn extract_split_reversal_flags(payment: &Payment) -> Vec<bool> {
    payment
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
        .collect()
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
/// let _ = client.refunds().create(RefundCreateParams::new("pay_123")).await;
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
    /// Accepts either an owned [`RefundCreateParams`] or `&RefundCreateParams`.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError`] on invalid payment IDs, HTTP/API errors, or
    /// invalid refund amounts.
    pub async fn create(
        &self,
        params: impl Borrow<RefundCreateParams>,
    ) -> Result<RefundReceipt, MoyasarError> {
        let params = params.borrow();
        let id = PaymentId::new(&params.payment_id)?;
        let payments = self.client.payments();
        let payment = payments.retrieve(&id).await?;
        validate_refund_amount(&payment, params.amount)?;

        let route = select_refund_route(
            &payment,
            params.amount,
            params.prefer_void_if_unsettled,
            Utc::now(),
        );
        let reversed = match route {
            RefundRoute::Void => self.void_with_refund_fallback(&payment, params.amount).await?,
            RefundRoute::Refund { amount } => payments.refund_payment(&payment, amount).await?,
        };
        Ok(RefundReceipt::from_payment(&reversed))
    }

    async fn void_with_refund_fallback(
        &self,
        payment: &Payment,
        amount: Option<u64>,
    ) -> Result<Payment, MoyasarError> {
        let payments = self.client.payments();
        match payments.void(&payment.id).await {
            Ok(voided) => Ok(voided),
            Err(err) if is_void_fallback_eligible(payment, &err) => {
                payments.refund_payment(payment, amount).await
            }
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};
    use serde_json::{Value, json};
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::error::ApiErrorType;

    fn mock_payment_json(id: &str, status: &str, refunded: u64, created_at: &str) -> Value {
        let refunded_fmt = match refunded {
            5000 => "50.00 SAR",
            15000 => "150.00 SAR",
            _ => "0.00 SAR",
        };
        json!({
            "id": id,
            "status": status,
            "amount": 15000,
            "amount_format": "150.00 SAR",
            "fee": 0,
            "fee_format": "0.00 SAR",
            "refunded": refunded,
            "refunded_format": refunded_fmt,
            "captured": if status == "voided" { 0 } else { 15000 },
            "captured_format": if status == "voided" { "0.00 SAR" } else { "150.00 SAR" },
            "currency": "SAR",
            "source": {"type": "creditcard"},
            "refunded_at": (status == "refunded").then_some(created_at),
            "voided_at": (status == "voided").then_some(created_at),
            "created_at": created_at,
            "updated_at": created_at
        })
    }

    async fn mount_recent_paid_payment(server: &MockServer, id: &str) -> String {
        let recent = (Utc::now() - Duration::minutes(15)).to_rfc3339();
        Mock::given(method("GET"))
            .and(path(format!("/v1/payments/{id}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(mock_payment_json(id, "paid", 0, &recent)),
            )
            .mount(server)
            .await;
        recent
    }

    #[test]
    fn synthesizes_refund_and_void_receipts_with_currency_and_split_flags() {
        let now = Utc.with_ymd_and_hms(2026, 11, 14, 10, 0, 0).unwrap();
        let mut payment: Payment = serde_json::from_value(mock_payment_json(
            "pay_01J9TEST",
            "refunded",
            5000,
            &now.to_rfc3339(),
        ))
        .unwrap();
        payment.splits = Some(vec![json!({"refunded": true}), json!({"refunded": false})]);

        let receipt = RefundReceipt::from_payment(&payment);
        assert_eq!(receipt.payment_id, "pay_01J9TEST");
        assert_eq!(receipt.currency, Currency::SAR);
        assert_eq!(receipt.refunded_amount, 5000);
        assert_eq!(receipt.refunded_format, "50.00 SAR");
        assert_eq!(receipt.split_reversal_flags, vec![true, false]);

        let voided: Payment = serde_json::from_value(mock_payment_json(
            "pay_01J9VOID",
            "voided",
            0,
            &now.to_rfc3339(),
        ))
        .unwrap();
        let void_receipt = RefundReceipt::from_payment(&voided);
        assert_eq!(void_receipt.refunded_amount, 15000);
        assert_eq!(void_receipt.refunded_format, "150.00 SAR");
    }

    #[tokio::test]
    async fn wiremock_refunds_service_handles_partial_and_unsettled_void() {
        let server = MockServer::start().await;
        let recent = mount_recent_paid_payment(&server, "pay_ref_1").await;
        Mock::given(method("POST"))
            .and(path("/v1/payments/pay_ref_1/refund"))
            .and(body_json(json!({"amount": 5000})))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(mock_payment_json("pay_ref_1", "refunded", 5000, &recent)),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/payments/pay_ref_1/void"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(mock_payment_json("pay_ref_1", "voided", 0, &recent)),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = MoyasarClient::builder(SecretKey::new("sk_test_123").unwrap())
            .base_url(server.uri())
            .build()
            .unwrap();
        let partial = client
            .refunds()
            .create(RefundCreateParams::new("pay_ref_1").amount(5000))
            .await
            .unwrap();
        assert_eq!(partial.refunded_amount, 5000);
        assert_eq!(partial.currency, Currency::SAR);

        let voided = client
            .refunds()
            .create(&RefundCreateParams::new("pay_ref_1"))
            .await
            .unwrap();
        assert_eq!(voided.status, PaymentStatus::Voided);
        assert_eq!(voided.refunded_amount, 15000);
        assert_eq!(voided.refunded_format, "150.00 SAR");
    }

    #[tokio::test]
    async fn wiremock_refunds_service_falls_back_on_settled_void_and_rejects_over_refund() {
        let server = MockServer::start().await;
        let recent = mount_recent_paid_payment(&server, "pay_settled").await;
        Mock::given(method("POST"))
            .and(path("/v1/payments/pay_settled/void"))
            .respond_with(ResponseTemplate::new(422).set_body_json(json!({
                "type": "invalid_request_error",
                "message": "Payment already settled"
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/payments/pay_settled/refund"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(mock_payment_json("pay_settled", "refunded", 15000, &recent)),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = MoyasarClient::builder(SecretKey::new("sk_test_123").unwrap())
            .base_url(server.uri())
            .build()
            .unwrap();
        let fallback = client
            .refunds()
            .create(RefundCreateParams::new("pay_settled"))
            .await
            .unwrap();
        assert_eq!(fallback.status, PaymentStatus::Refunded);
        assert_eq!(fallback.refunded_amount, 15000);

        let over_err = client
            .refunds()
            .create(RefundCreateParams::new("pay_settled").amount(20000))
            .await
            .expect_err("over-refund must be rejected pre-network");
        assert!(matches!(
            over_err,
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            }
        ));
    }
}
