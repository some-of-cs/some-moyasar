//! E2E tests for payment creation, `InMemoryCustomerVault`, and `customer_id` metadata injection.

use actix_web::http::StatusCode;
use actix_web::{App, test};
use serde_json::{Value, json};
use some_moyasar::prelude::{CustomerVault, FIXTURE_TIMESTAMP, TokenId};
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{build_test_harness, configure_test_routes};

/// Verifies `POST /v1/customers/cus_actix_1/tokens` associates `token_mock_01` in `InMemoryCustomerVault`.
#[actix_rt::test]
async fn attach_customer_token_stores_token_in_vault() {
    let (_mock_server, app_state) = build_test_harness().await;
    let vault = app_state.vault.clone();
    let app = test::init_service(
        App::new()
            .app_data(app_state)
            .configure(configure_test_routes),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/v1/customers/cus_actix_1/tokens")
        .set_json(json!({ "token_id": "token_mock_01" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["customer_id"], "cus_actix_1");
    assert_eq!(body["token_id"], "token_mock_01");

    let tokens = vault
        .resolve_tokens("cus_actix_1")
        .await
        .expect("vault lookup should succeed");
    let expected = TokenId::new("token_mock_01").expect("valid token id");
    assert_eq!(tokens, vec![expected]);
}

/// Verifies `POST /v1/payments` with `customer_id: Some("cus_actix_1")` injects `metadata["customer_id"]`.
#[actix_rt::test]
async fn create_payment_injects_customer_id_metadata() {
    let (mock_server, app_state) = build_test_harness().await;
    Mock::given(method("POST"))
        .and(path("/v1/payments"))
        .and(body_partial_json(json!({
            "amount": 15000,
            "currency": "SAR",
            "metadata": { "customer_id": "cus_actix_1" }
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": "pay_mock_01",
            "status": "paid",
            "amount": 15000,
            "amount_format": "150.00 SAR",
            "fee": 0,
            "fee_format": "0.00 SAR",
            "refunded": 0,
            "refunded_format": "0.00 SAR",
            "captured": 15000,
            "captured_format": "150.00 SAR",
            "currency": "SAR",
            "source": { "type": "token" },
            "metadata": { "customer_id": "cus_actix_1" },
            "created_at": FIXTURE_TIMESTAMP,
            "updated_at": FIXTURE_TIMESTAMP
        })))
        .with_priority(1)
        .expect(1)
        .mount(mock_server.server())
        .await;

    let app = test::init_service(
        App::new()
            .app_data(app_state)
            .configure(configure_test_routes),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/v1/payments")
        .set_json(json!({
            "amount": 15000,
            "currency": "SAR",
            "token_id": "token_mock_01",
            "customer_id": "cus_actix_1"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["id"], "pay_mock_01");
    assert_eq!(body["metadata"]["customer_id"], "cus_actix_1");
}

/// Verifies `GET /v1/payments/pay_mock_01` returns `200 OK` with the mocked payment payload.
#[actix_rt::test]
async fn get_payment_returns_200_ok() {
    let (_mock_server, app_state) = build_test_harness().await;
    let app = test::init_service(
        App::new()
            .app_data(app_state)
            .configure(configure_test_routes),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/v1/payments/pay_mock_01")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["id"], "pay_mock_01");
    assert_eq!(body["status"], "paid");
}

/// Verifies invalid token prefix returns 4xx without leaking `sk_test_` secret keys.
#[actix_rt::test]
async fn invalid_token_prefix_returns_4xx_without_secret_leak() {
    let (_mock_server, app_state) = build_test_harness().await;
    let app = test::init_service(
        App::new()
            .app_data(app_state)
            .configure(configure_test_routes),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/v1/payments")
        .set_json(json!({
            "amount": 15000,
            "currency": "SAR",
            "token_id": "bad_prefix_01"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status().is_client_error(),
        "expected 4xx client error, got {}",
        resp.status()
    );

    let raw_body = test::read_body(resp).await;
    let body_text = std::str::from_utf8(&raw_body).expect("error response should be valid UTF-8");
    assert!(!body_text.contains("sk_test_"));
}
