//! Subscription cycle refund and void-fallback orchestration.

use chrono::Utc;

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::resources::payments::{PaymentId, can_void_payment};
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
        let payments = self.client().payments();
        let payment = payments.retrieve(&id).await?;
        let now = Utc::now();

        match params.strategy {
            RefundStrategy::Full if can_void_payment(&payment, now) => {
                let voided = payments.void(id.as_str()).await?;
                Ok(SubscriptionCycleRefundResult {
                    payment_id: voided.id,
                    action: ReversalAction::Voided,
                    amount_reversed: voided.amount,
                })
            }
            RefundStrategy::Full => {
                let refunded = payments.refund(id.as_str(), None).await?;
                Ok(SubscriptionCycleRefundResult {
                    payment_id: refunded.id,
                    action: ReversalAction::Refunded,
                    amount_reversed: refunded.refunded,
                })
            }
            RefundStrategy::Partial { amount_halalas } => {
                let refunded = payments.refund(id.as_str(), Some(amount_halalas)).await?;
                Ok(SubscriptionCycleRefundResult {
                    payment_id: refunded.id,
                    action: ReversalAction::Refunded,
                    amount_reversed: refunded.refunded,
                })
            }
            RefundStrategy::ProRata { canceled_at } => {
                let amount = pro_rata_refund(
                    payment.amount,
                    params.period_start,
                    params.period_end,
                    canceled_at,
                )?;
                let refunded = payments.refund(id.as_str(), Some(amount)).await?;
                Ok(SubscriptionCycleRefundResult {
                    payment_id: refunded.id,
                    action: ReversalAction::Refunded,
                    amount_reversed: refunded.refunded,
                })
            }
        }
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

    #[tokio::test]
    async fn tc_8_2_0_04_full_within_2h_voids_and_pro_rata_refunds() {
        let server = MockServer::start().await;
        let now = Utc::now();
        let recent_created = (now - Duration::minutes(20)).to_rfc3339();

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
                "created_at": recent_created,
                "updated_at": recent_created
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/payments/pay_recent/void"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "pay_recent",
                "status": "voided",
                "amount": 29900,
                "amount_format": "299.00 SAR",
                "fee": 0,
                "fee_format": "0.00 SAR",
                "refunded": 0,
                "refunded_format": "0.00 SAR",
                "captured": 0,
                "captured_format": "0.00 SAR",
                "currency": "SAR",
                "source": {"type": "token"},
                "voided_at": now.to_rfc3339(),
                "created_at": recent_created,
                "updated_at": now.to_rfc3339()
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/payments/pay_recent/refund"))
            .and(body_json(json!({"amount": 19934})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "pay_recent",
                "status": "refunded",
                "amount": 29900,
                "amount_format": "299.00 SAR",
                "fee": 0,
                "fee_format": "0.00 SAR",
                "refunded": 19934,
                "refunded_format": "199.34 SAR",
                "captured": 29900,
                "captured_format": "299.00 SAR",
                "currency": "SAR",
                "source": {"type": "token"},
                "refunded_at": now.to_rfc3339(),
                "created_at": recent_created,
                "updated_at": now.to_rfc3339()
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = MoyasarClient::builder(SecretKey::new("sk_test_123").unwrap())
            .base_url(server.uri())
            .build()
            .unwrap();

        let start = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
        let end = start + Duration::days(30);
        let void_res = client
            .subscriptions()
            .refund_cycle(&SubscriptionCycleRefundParams {
                subscription_id: "sub_99".to_owned(),
                cycle_payment_id: "pay_recent".to_owned(),
                period_start: start,
                period_end: end,
                strategy: RefundStrategy::Full,
            })
            .await
            .unwrap();

        assert_eq!(void_res.action, ReversalAction::Voided);
        assert_eq!(void_res.amount_reversed, 29900);

        let prorata_res = client
            .subscriptions()
            .refund_cycle(&SubscriptionCycleRefundParams {
                subscription_id: "sub_99".to_owned(),
                cycle_payment_id: "pay_recent".to_owned(),
                period_start: start,
                period_end: end,
                strategy: RefundStrategy::ProRata {
                    canceled_at: start + Duration::days(10),
                },
            })
            .await
            .unwrap();

        assert_eq!(prorata_res.action, ReversalAction::Refunded);
        assert_eq!(prorata_res.amount_reversed, 19934);
    }
}
