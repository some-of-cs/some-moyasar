//! Smart refund-or-void routing strategy for the unsettled 2-hour window.

use chrono::{DateTime, Utc};

use crate::resources::payments::{Payment, can_void_payment};

/// Selected Moyasar reversal endpoint for a [`RefundCreateParams`](super::refunds::RefundCreateParams).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefundRoute {
    /// Route to `POST /v1/payments/:id/void` (full reversal within unsettled window).
    Void,
    /// Route to `POST /v1/payments/:id/refund` with an optional partial amount.
    Refund {
        /// Optional partial refund amount in minor units.
        amount: Option<u64>,
    },
}

/// Selects `/void` only for full reversals within the 2-hour unsettled window when requested.
#[must_use]
pub fn select_refund_route(
    payment: &Payment,
    requested_amount: Option<u64>,
    prefer_void_if_unsettled: bool,
    now: DateTime<Utc>,
) -> RefundRoute {
    let is_full_unrefunded =
        payment.refunded == 0 && requested_amount.is_none_or(|amt| amt == payment.amount);
    if prefer_void_if_unsettled && is_full_unrefunded && can_void_payment(payment, now) {
        return RefundRoute::Void;
    }
    RefundRoute::Refund {
        amount: requested_amount,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};
    use serde_json::{Value, json};
    use wiremock::matchers::{body_json, body_string, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::client::MoyasarClient;
    use crate::kernel::types::Currency;
    use crate::resources::payments::PaymentStatus;

    fn sample_payment(created_at: DateTime<Utc>, refunded: u64) -> Payment {
        Payment {
            id: "pay_01J9ROUTE".to_owned(),
            status: PaymentStatus::Paid,
            amount: 15000,
            amount_format: "150.00 SAR".to_owned(),
            fee: 0,
            fee_format: "0.00 SAR".to_owned(),
            refunded,
            refunded_format: "0.00 SAR".to_owned(),
            captured: 15000,
            captured_format: "150.00 SAR".to_owned(),
            currency: Currency::SAR,
            description: None,
            source: json!({"type": "creditcard"}),
            metadata: Default::default(),
            splits: None,
            callback_url: None,
            invoice_id: None,
            ip: None,
            captured_at: Some(created_at),
            refunded_at: None,
            voided_at: None,
            created_at,
            updated_at: created_at,
        }
    }

    fn route_payment_json(id: &str, status: &str, refunded: u64) -> Value {
        json!({
            "id": id,
            "status": status,
            "amount": 15000,
            "amount_format": "150.00 SAR",
            "fee": 0,
            "fee_format": "0.00 SAR",
            "refunded": refunded,
            "refunded_format": if refunded > 0 { "50.00 SAR" } else { "0.00 SAR" },
            "captured": if status == "voided" { 0 } else { 15000 },
            "captured_format": if status == "voided" { "0.00 SAR" } else { "150.00 SAR" },
            "currency": "SAR",
            "source": {"type": "creditcard"},
            "refunded_at": (status == "refunded").then_some("2026-11-14T12:00:00Z"),
            "voided_at": (status == "voided").then_some("2026-11-14T12:00:00Z"),
            "created_at": "2026-11-14T11:30:00Z",
            "updated_at": "2026-11-14T12:00:00Z"
        })
    }

    #[test]
    fn tc_8_1_0_02_routes_unsettled_full_to_void_and_partial_or_refunded_to_refund() {
        let now = Utc.with_ymd_and_hms(2026, 11, 14, 12, 0, 0).unwrap();
        let recent = sample_payment(now - Duration::minutes(30), 0);
        let partially_refunded = sample_payment(now - Duration::minutes(30), 5000);
        let settled = sample_payment(now - Duration::hours(3), 0);

        assert_eq!(
            select_refund_route(&recent, None, true, now),
            RefundRoute::Void
        );
        assert_eq!(
            select_refund_route(&recent, Some(15000), true, now),
            RefundRoute::Void
        );
        assert_eq!(
            select_refund_route(&recent, Some(5000), true, now),
            RefundRoute::Refund { amount: Some(5000) }
        );
        assert_eq!(
            select_refund_route(&partially_refunded, None, true, now),
            RefundRoute::Refund { amount: None }
        );
        assert_eq!(
            select_refund_route(&settled, None, true, now),
            RefundRoute::Refund { amount: None }
        );
        assert_eq!(
            select_refund_route(&recent, None, false, now),
            RefundRoute::Refund { amount: None }
        );
    }

    #[tokio::test]
    async fn tc_8_1_0_01_wiremock_partial_refund_and_void_dispatch() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/payments/pay_01J9ROUTE/refund"))
            .and(body_json(json!({"amount": 5000})))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(route_payment_json("pay_01J9ROUTE", "refunded", 5000)),
            )
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/payments/pay_01J9VOID/void"))
            .and(body_string(""))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(route_payment_json("pay_01J9VOID", "voided", 0)),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = MoyasarClient::builder(SecretKey::new("sk_test_123").unwrap())
            .base_url(server.uri())
            .build()
            .unwrap();
        let refunded = client
            .payments()
            .refund("pay_01J9ROUTE", Some(5000))
            .await
            .unwrap();
        assert_eq!(refunded.refunded, 5000);
        let voided = client.payments().void("pay_01J9VOID").await.unwrap();
        assert_eq!(voided.status, PaymentStatus::Voided);
    }
}
