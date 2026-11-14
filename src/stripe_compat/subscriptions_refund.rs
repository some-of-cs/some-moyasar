//! Subscription cycle refund and void-fallback orchestration.

use chrono::{DateTime, Utc};

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::resources::payments::{Payment, PaymentId, can_void_payment};
use crate::stripe_compat::pro_rata::pro_rata_refund;
use crate::stripe_compat::subscriptions_billing::SubscriptionsService;
use crate::stripe_compat::subscriptions_model::{
    RefundStrategy, ReversalAction, SubscriptionCycleRefundParams, SubscriptionCycleRefundResult,
};

impl<K: SecretAuthKey> SubscriptionsService<'_, K> {
    /// Refunds or voids a subscription cycle payment according to [`RefundStrategy`].
    ///
    /// Full reversals within the ~2-hour unsettled window route to `POST /v1/payments/:id/void`;
    /// partial, pro-rata, and settled reversals route to `POST /v1/payments/:id/refund`.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError`] on invalid payment IDs, invalid period bounds, or API errors.
    pub async fn refund_cycle(
        &self,
        params: &SubscriptionCycleRefundParams,
    ) -> Result<SubscriptionCycleRefundResult, MoyasarError> {
        let id = PaymentId::new(&params.cycle_payment_id)?;
        let payment = self.client().payments().retrieve(&id).await?;

        match params.strategy {
            RefundStrategy::Full => self.void_or_refund_full(&id, &payment).await,
            RefundStrategy::Partial { amount_halalas } => {
                self.refund_partial(&id, amount_halalas).await
            }
            RefundStrategy::ProRata { canceled_at } => {
                self.refund_pro_rata(&id, &payment, params, canceled_at).await
            }
        }
    }

    async fn void_or_refund_full(
        &self,
        id: &PaymentId,
        payment: &Payment,
    ) -> Result<SubscriptionCycleRefundResult, MoyasarError> {
        let payments = self.client().payments();
        if can_void_payment(payment, Utc::now()) {
            let voided = payments.void(id.as_str()).await?;
            return Ok(SubscriptionCycleRefundResult {
                payment_id: voided.id,
                action: ReversalAction::Voided,
                amount_reversed: voided.amount,
            });
        }
        let refunded = payments.refund(id.as_str(), None).await?;
        Ok(SubscriptionCycleRefundResult {
            payment_id: refunded.id,
            action: ReversalAction::Refunded,
            amount_reversed: refunded.refunded,
        })
    }

    async fn refund_partial(
        &self,
        id: &PaymentId,
        amount_halalas: u64,
    ) -> Result<SubscriptionCycleRefundResult, MoyasarError> {
        let refunded = self
            .client()
            .payments()
            .refund(id.as_str(), Some(amount_halalas))
            .await?;
        Ok(SubscriptionCycleRefundResult {
            payment_id: refunded.id,
            action: ReversalAction::Refunded,
            amount_reversed: refunded.refunded,
        })
    }

    async fn refund_pro_rata(
        &self,
        id: &PaymentId,
        payment: &Payment,
        params: &SubscriptionCycleRefundParams,
        canceled_at: DateTime<Utc>,
    ) -> Result<SubscriptionCycleRefundResult, MoyasarError> {
        let amount = pro_rata_refund(
            payment.amount,
            params.period_start,
            params.period_end,
            canceled_at,
        )?;
        self.refund_partial(id, amount).await
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};
    use serde_json::json;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::client::MoyasarClient;

    async fn setup_recent_payment_server() -> (MockServer, MoyasarClient<SecretKey>, String) {
        let server = MockServer::start().await;
        let now = Utc::now();
        let recent = (now - Duration::minutes(20)).to_rfc3339();
        Mock::given(method("GET"))
            .and(path("/v1/payments/pay_recent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "pay_recent",
                "status": "paid",
                "amount": 29900,
                "amount_format": "299.00 SAR",
                "fee": 0,
                "fee_format": "0.00 SAR",
                "refunded": 0,
                "refunded_format": "0.00 SAR",
                "captured": 29900,
                "captured_format": "299.00 SAR",
                "currency": "SAR",
                "source": {"type": "token"},
                "created_at": recent,
                "updated_at": recent
            })))
            .mount(&server)
            .await;
        let client = MoyasarClient::builder(SecretKey::new("sk_test_123").unwrap())
            .base_url(server.uri())
            .build()
            .unwrap();
        (server, client, recent)
    }

    #[tokio::test]
    async fn tc_8_2_0_04_full_within_2h_routes_to_void() {
        let (server, client, recent) = setup_recent_payment_server().await;
        let now = Utc::now().to_rfc3339();
        Mock::given(method("POST"))
            .and(path("/v1/payments/pay_recent/void"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "pay_recent", "status": "voided", "amount": 29900,
                "amount_format": "299.00 SAR", "fee": 0, "fee_format": "0.00 SAR",
                "refunded": 0, "refunded_format": "0.00 SAR", "captured": 0,
                "captured_format": "0.00 SAR", "currency": "SAR",
                "source": {"type": "token"}, "voided_at": now,
                "created_at": recent, "updated_at": now
            })))
            .expect(1)
            .mount(&server)
            .await;

        let start = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
        let res = client
            .subscriptions()
            .refund_cycle(&SubscriptionCycleRefundParams {
                subscription_id: "sub_99".to_owned(),
                cycle_payment_id: "pay_recent".to_owned(),
                period_start: start,
                period_end: start + Duration::days(30),
                strategy: RefundStrategy::Full,
            })
            .await
            .unwrap();
        assert_eq!(res.action, ReversalAction::Voided);
        assert_eq!(res.amount_reversed, 29900);
    }

    #[tokio::test]
    async fn tc_8_2_0_04_pro_rata_routes_to_refund() {
        let (server, client, recent) = setup_recent_payment_server().await;
        let now = Utc::now().to_rfc3339();
        Mock::given(method("POST"))
            .and(path("/v1/payments/pay_recent/refund"))
            .and(body_json(json!({"amount": 19934})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "pay_recent", "status": "refunded", "amount": 29900,
                "amount_format": "299.00 SAR", "fee": 0, "fee_format": "0.00 SAR",
                "refunded": 19934, "refunded_format": "199.34 SAR", "captured": 29900,
                "captured_format": "299.00 SAR", "currency": "SAR",
                "source": {"type": "token"}, "refunded_at": now,
                "created_at": recent, "updated_at": now
            })))
            .expect(1)
            .mount(&server)
            .await;

        let start = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
        let res = client
            .subscriptions()
            .refund_cycle(&SubscriptionCycleRefundParams {
                subscription_id: "sub_99".to_owned(),
                cycle_payment_id: "pay_recent".to_owned(),
                period_start: start,
                period_end: start + Duration::days(30),
                strategy: RefundStrategy::ProRata {
                    canceled_at: start + Duration::days(10),
                },
            })
            .await
            .unwrap();
        assert_eq!(res.action, ReversalAction::Refunded);
        assert_eq!(res.amount_reversed, 19934);
    }
}
