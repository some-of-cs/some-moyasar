//! E2E tests for `CustomersService`, `SubscriptionsService`, and `UnifiedPaymentGateway` routes.
#![forbid(unsafe_code)]

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use chrono::{Duration, TimeZone, Utc};
use serde_json::{Value, json};
use some_moyasar::prelude::derive_subscription_given_id;
use some_moyasar::test_utils::fixtures::FIXTURE_TIMESTAMP;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::spawn_test_app;

/// Builds a deterministic active Moyasar token JSON payload.
fn active_token_json(id: &str) -> Value {
    json!({
        "id": id,
        "status": "active",
        "number": "411111XXXXXX1111",
        "company": "visa",
        "gateway_id": "gw_axum_test",
        "created_at": FIXTURE_TIMESTAMP
    })
}

/// Builds a Moyasar token payment JSON payload for customer, subscription, and unified charge tests.
fn token_payment_json(id: &str, status: &str, amount: u64, created_at: &str) -> Value {
    let captured = if status == "voided" { 0_u64 } else { amount };
    let captured_fmt = if status == "voided" {
        "0.00 SAR"
    } else {
        "150.00 SAR"
    };
    json!({
        "id": id,
        "status": status,
        "amount": amount,
        "amount_format": "150.00 SAR",
        "fee": 0,
        "fee_format": "0.00 SAR",
        "refunded": 0,
        "refunded_format": "0.00 SAR",
        "captured": captured,
        "captured_format": captured_fmt,
        "currency": "SAR",
        "source": {"type": "token"},
        "metadata": {"customer_id": "cus_axum_101", "order_id": "ord_101"},
        "voided_at": (status == "voided").then_some(created_at),
        "created_at": created_at,
        "updated_at": created_at
    })
}

/// Sends a JSON HTTP request through the Axum router via `tower::ServiceExt::oneshot`.
async fn dispatch_json(
    app: Router,
    http_method: Method,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    let bytes = serde_json::to_vec(&body).expect("serialize request json");
    let req = Request::builder()
        .method(http_method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(bytes))
        .expect("build HTTP request");
    let resp = app.oneshot(req).await.expect("oneshot request succeeds");
    let status = resp.status();
    let out_bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let parsed = serde_json::from_slice(&out_bytes).expect("parse response json");
    (status, parsed)
}

/// Verifies `CustomersService` (`POST /customers/{id}/tokens` + `POST /customers/{id}/charge`).
#[tokio::test]
async fn e2e_customers_attach_token_and_charge() {
    let (mock, router) = spawn_test_app().await;
    let given_id = Uuid::new_v4();

    let (attach_status, attach_body) = dispatch_json(
        router.clone(),
        Method::POST,
        "/customers/cus_axum_101/tokens",
        json!({ "token_id": "token_cus_101" }),
    )
    .await;
    assert_eq!(attach_status, StatusCode::CREATED);
    assert_eq!(attach_body["customer_id"], "cus_axum_101");
    assert_eq!(attach_body["token_id"], "token_cus_101");
    assert_eq!(attach_body["status"], "attached");

    let upstream = token_payment_json("pay_cus_101", "paid", 15000, FIXTURE_TIMESTAMP);
    Mock::given(method("POST"))
        .and(path("/v1/payments"))
        .and(body_partial_json(json!({
            "amount": 15000,
            "currency": "SAR",
            "given_id": given_id,
            "metadata": {"customer_id": "cus_axum_101", "order_id": "ord_101"},
            "source": {"type": "token", "token": "token_cus_101", "3ds": false}
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(upstream))
        .expect(1)
        .mount(mock.server())
        .await;

    let (charge_status, charge_body) = dispatch_json(
        router,
        Method::POST,
        "/customers/cus_axum_101/charge",
        json!({
            "amount": 15000,
            "currency": "SAR",
            "given_id": given_id,
            "metadata": { "order_id": "ord_101" }
        }),
    )
    .await;
    assert_eq!(charge_status, StatusCode::CREATED);
    assert_eq!(charge_body["id"], "pay_cus_101");
    assert_eq!(charge_body["status"], "paid");
    assert_eq!(charge_body["metadata"]["customer_id"], "cus_axum_101");
    assert_eq!(charge_body["metadata"]["order_id"], "ord_101");
}

/// Verifies `SubscriptionsService` (`POST /subscriptions/{id}/charge` + `POST /subscriptions/{id}/refund`).
#[tokio::test]
async fn e2e_subscriptions_charge_cycle_and_refund_cycle() {
    let (mock, router) = spawn_test_app().await;
    let recent_ts = (Utc::now() - Duration::minutes(15)).to_rfc3339();
    let expected_given_id = derive_subscription_given_id("sub_axum_99", 2);

    Mock::given(method("GET"))
        .and(path("/v1/tokens/token_sub_01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(active_token_json("token_sub_01")))
        .expect(1)
        .mount(mock.server())
        .await;

    let paid = token_payment_json("pay_sub_cycle_2", "paid", 29900, &recent_ts);
    Mock::given(method("POST"))
        .and(path("/v1/payments"))
        .and(body_partial_json(json!({
            "amount": 29900,
            "currency": "SAR",
            "given_id": expected_given_id,
            "source": {"type": "token", "token": "token_sub_01", "3ds": false}
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(paid.clone()))
        .expect(1)
        .mount(mock.server())
        .await;

    let (charge_status, charge_body) = dispatch_json(
        router.clone(),
        Method::POST,
        "/subscriptions/sub_axum_99/charge",
        json!({
            "cycle_index": 2,
            "amount": 29900,
            "currency": "SAR",
            "token_id": "token_sub_01"
        }),
    )
    .await;
    assert_eq!(charge_status, StatusCode::CREATED);
    assert_eq!(charge_body["id"], "pay_sub_cycle_2");
    assert_eq!(charge_body["amount"], 29900);

    let voided = token_payment_json("pay_sub_cycle_2", "voided", 29900, &recent_ts);
    Mock::given(method("GET"))
        .and(path("/v1/payments/pay_sub_cycle_2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(paid))
        .expect(1)
        .mount(mock.server())
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/payments/pay_sub_cycle_2/void"))
        .respond_with(ResponseTemplate::new(200).set_body_json(voided))
        .expect(1)
        .mount(mock.server())
        .await;

    let start = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
    let end = start + Duration::days(30);
    let (refund_status, refund_body) = dispatch_json(
        router,
        Method::POST,
        "/subscriptions/sub_axum_99/refund",
        json!({
            "cycle_payment_id": "pay_sub_cycle_2",
            "period_start": start.to_rfc3339(),
            "period_end": end.to_rfc3339(),
            "strategy": "Full"
        }),
    )
    .await;
    assert_eq!(refund_status, StatusCode::OK);
    assert_eq!(refund_body["payment_id"], "pay_sub_cycle_2");
    assert_eq!(refund_body["action"], "Voided");
    assert_eq!(refund_body["amount_reversed"], 29900);
}

/// Verifies `UnifiedPaymentGateway` (`POST /unified/charge_token` + `POST /unified/reversal`).
#[tokio::test]
async fn e2e_unified_gateway_charge_saved_token_and_refund_or_void() {
    let (mock, router) = spawn_test_app().await;
    let recent_ts = (Utc::now() - Duration::minutes(10)).to_rfc3339();
    let idem_key = Uuid::new_v4();

    Mock::given(method("GET"))
        .and(path("/v1/tokens/token_uni_01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(active_token_json("token_uni_01")))
        .expect(1)
        .mount(mock.server())
        .await;

    let paid = token_payment_json("pay_uni_01", "paid", 15000, &recent_ts);
    Mock::given(method("POST"))
        .and(path("/v1/payments"))
        .and(body_partial_json(json!({
            "amount": 15000,
            "currency": "SAR",
            "given_id": idem_key,
            "source": {"type": "token", "token": "token_uni_01", "3ds": false}
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(paid.clone()))
        .expect(1)
        .mount(mock.server())
        .await;

    let (charge_status, charge_body) = dispatch_json(
        router.clone(),
        Method::POST,
        "/unified/charge_token",
        json!({
            "amount_minor": 15000,
            "currency": "SAR",
            "token_id": "token_uni_01",
            "idempotency_key": idem_key
        }),
    )
    .await;
    assert_eq!(charge_status, StatusCode::CREATED);
    assert_eq!(charge_body["transaction_id"], "pay_uni_01");
    assert_eq!(charge_body["status"], "paid");
    assert_eq!(charge_body["amount_minor"], 15000);
    assert_eq!(charge_body["currency"], "SAR");

    let voided = token_payment_json("pay_uni_01", "voided", 15000, &recent_ts);
    Mock::given(method("GET"))
        .and(path("/v1/payments/pay_uni_01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(paid))
        .expect(1)
        .mount(mock.server())
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/payments/pay_uni_01/void"))
        .respond_with(ResponseTemplate::new(200).set_body_json(voided))
        .expect(1)
        .mount(mock.server())
        .await;

    let (rev_status, rev_body) = dispatch_json(
        router,
        Method::POST,
        "/unified/reversal",
        json!({
            "transaction_id": "pay_uni_01",
            "amount_minor": null
        }),
    )
    .await;
    assert_eq!(rev_status, StatusCode::OK);
    assert_eq!(rev_body["transaction_id"], "pay_uni_01");
    assert_eq!(rev_body["action"], "voided");
    assert_eq!(rev_body["amount_reversed_minor"], 15000);
    assert_eq!(rev_body["currency"], "SAR");
}
