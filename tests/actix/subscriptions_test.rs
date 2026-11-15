//! E2E tests for deterministic UUIDv5 subscription billing and integer-ceiling pro-rata refunds.

use actix_web::{App, test};
use serde_json::{Value, json};
use some_moyasar::prelude::derive_subscription_given_id;
use wiremock::matchers::{body_json, body_partial_json, method, path};
use wiremock::{Mock, ResponseTemplate};

const MOCK_CREATED_AT: &str = "2026-11-01T00:00:00Z";

/// Builds a deterministic mock payment JSON payload for subscription E2E tests.
fn subscription_payment_json(id: &str, status: &str, refunded: u64) -> Value {
    let refunded_fmt = if refunded > 0 {
        "199.34 SAR"
    } else {
        "0.00 SAR"
    };
    json!({
        "id": id,
        "status": status,
        "amount": 29900,
        "amount_format": "299.00 SAR",
        "fee": 0,
        "fee_format": "0.00 SAR",
        "refunded": refunded,
        "refunded_format": refunded_fmt,
        "captured": 29900,
        "captured_format": "299.00 SAR",
        "currency": "SAR",
        "source": { "type": "token" },
        "refunded_at": (status == "refunded").then_some(MOCK_CREATED_AT),
        "created_at": MOCK_CREATED_AT,
        "updated_at": MOCK_CREATED_AT
    })
}

/// Mounts an active token responder on `GET /v1/tokens/token_mock_01`.
async fn mount_active_token_mock(mock_server: &some_moyasar::test_utils::MoyasarMockServer) {
    Mock::given(method("GET"))
        .and(path("/v1/tokens/token_mock_01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "token_mock_01",
            "status": "active",
            "number": "411111XXXXXX1111",
            "company": "visa",
            "gateway_id": "gw_mock",
            "created_at": MOCK_CREATED_AT
        })))
        .with_priority(1)
        .mount(mock_server.server())
        .await;
}

/// Mounts a 29,900-halala payment responder and a strict pro-rata refund responder on `mock_server`.
async fn mount_pro_rata_refund_mocks(mock_server: &some_moyasar::test_utils::MoyasarMockServer) {
    Mock::given(method("GET"))
        .and(path("/v1/payments/pay_mock_01"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(subscription_payment_json(
                "pay_mock_01",
                "paid",
                0,
            )),
        )
        .with_priority(1)
        .mount(mock_server.server())
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/payments/pay_mock_01/refund"))
        .and(body_json(json!({ "amount": 19934 })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(subscription_payment_json(
                "pay_mock_01",
                "refunded",
                19934,
            )),
        )
        .with_priority(1)
        .expect(1)
        .mount(mock_server.server())
        .await;
}

/// Verifies `POST /v1/subscriptions/sub_actix_99/charge` sends deterministic UUIDv5 `given_id`.
#[actix_rt::test]
async fn subscription_charge_sends_deterministic_uuidv5_given_id() {
    let (mock_server, app_state) = super::build_test_harness().await;
    let expected_given_id = derive_subscription_given_id("sub_actix_99", 4);

    mount_active_token_mock(&mock_server).await;
    Mock::given(method("POST"))
        .and(path("/v1/payments"))
        .and(body_partial_json(json!({
            "given_id": expected_given_id,
            "amount": 29900
        })))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(subscription_payment_json(
                "pay_sub_actix_99_4",
                "paid",
                0,
            )),
        )
        .with_priority(1)
        .expect(1)
        .mount(mock_server.server())
        .await;

    let app = test::init_service(
        App::new()
            .app_data(app_state)
            .configure(super::configure_test_routes),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/v1/subscriptions/sub_actix_99/charge")
        .set_json(json!({
            "cycle_index": 4,
            "amount": 29900,
            "token_id": "token_mock_01"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success(), "expected 2xx, got {}", resp.status());

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["id"], "pay_sub_actix_99_4");
}

/// Verifies `POST /v1/subscriptions/sub_actix_99/refund` computes integer-ceiling pro-rata `19934`.
#[actix_rt::test]
async fn subscription_pro_rata_refund_returns_integer_ceiling_amount() {
    let (mock_server, app_state) = super::build_test_harness().await;
    mount_pro_rata_refund_mocks(&mock_server).await;

    let app = test::init_service(
        App::new()
            .app_data(app_state)
            .configure(super::configure_test_routes),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/v1/subscriptions/sub_actix_99/refund")
        .set_json(json!({
            "cycle_payment_id": "pay_mock_01",
            "period_start": "2026-11-01T00:00:00Z",
            "period_end": "2026-12-01T00:00:00Z",
            "canceled_at": "2026-11-11T00:00:00Z"
        }))
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success(), "expected 2xx, got {}", resp.status());

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["amount_reversed"], 19934);
}
