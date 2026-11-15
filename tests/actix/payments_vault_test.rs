//! E2E tests for payments, refunds, customer vault, tokens, intents, invoices, and 3DS card auths.

use actix_web::http::StatusCode;
use actix_web::{App, test};
use serde_json::{Value, json};
use some_moyasar::prelude::{CustomerVault, FIXTURE_TIMESTAMP, TokenId};
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{build_test_harness, configure_test_routes, mount_json_endpoint, paginated_envelope};

/// Builds a mock payment JSON payload with the given `id` and `status`.
fn payment_fixture(id: &str, status: &str, refunded: u64) -> Value {
    json!({
        "id": id,
        "status": status,
        "amount": 15000,
        "amount_format": "150.00 SAR",
        "fee": 0,
        "fee_format": "0.00 SAR",
        "refunded": refunded,
        "refunded_format": if refunded > 0 { "150.00 SAR" } else { "0.00 SAR" },
        "captured": 15000,
        "captured_format": "150.00 SAR",
        "currency": "SAR",
        "source": { "type": "token" },
        "metadata": { "customer_id": "cus_actix_1" },
        "created_at": FIXTURE_TIMESTAMP,
        "updated_at": FIXTURE_TIMESTAMP
    })
}

/// Mounts token, intent, invoice, and 3DS card-auth mocks on `mock_server`.
async fn mount_extended_payment_mocks(mock_server: &some_moyasar::prelude::MoyasarMockServer) {
    mount_json_endpoint(
        mock_server,
        ("GET", "/v1/payments"),
        200,
        paginated_envelope(json!([payment_fixture("pay_mock_01", "paid", 0)])),
    )
    .await;
    mount_json_endpoint(
        mock_server,
        ("POST", "/v1/tokens"),
        201,
        json!({
            "id": "token_new_01",
            "status": "active",
            "number": "411111XXXXXX1111",
            "company": "visa",
            "created_at": FIXTURE_TIMESTAMP
        }),
    )
    .await;
    mount_json_endpoint(
        mock_server,
        ("DELETE", "/v1/tokens/token_mock_01"),
        204,
        json!({}),
    )
    .await;
}

/// Mounts intent retrieval, cancel, fulfill, invoice, and card-auth mocks on `mock_server`.
async fn mount_intent_invoice_and_auth_mocks(
    mock_server: &some_moyasar::prelude::MoyasarMockServer,
) {
    let intent_json = json!({
        "id": "pi_mock_01",
        "status": "created",
        "amount": 15000,
        "currency": "SAR",
        "client_secret": "csec_actix_secret_123456",
        "expires_at": FIXTURE_TIMESTAMP,
        "created_at": FIXTURE_TIMESTAMP
    });
    mount_json_endpoint(
        mock_server,
        ("GET", "/v1/payment_intents/pi_mock_01"),
        200,
        intent_json.clone(),
    )
    .await;
    mount_json_endpoint(
        mock_server,
        ("POST", "/v1/payment_intents/pi_mock_01/cancel"),
        200,
        intent_json,
    )
    .await;
    mount_json_endpoint(
        mock_server,
        ("POST", "/v1/payment_intents/pi_mock_01/fulfill"),
        200,
        json!({
            "id": "pay_fulfilled_01",
            "status": "paid",
            "amount": 15000,
            "currency": "SAR",
            "created_at": FIXTURE_TIMESTAMP
        }),
    )
    .await;
    let inv_id = "00000000-0000-0000-0000-000000000001";
    let inv_json = json!({
        "id": inv_id,
        "status": "created",
        "amount": 50000,
        "currency": "SAR"
    });
    mount_json_endpoint(mock_server, ("POST", "/v1/invoices"), 201, inv_json.clone()).await;
    mount_json_endpoint(
        mock_server,
        ("GET", "/v1/invoices/00000000-0000-0000-0000-000000000001"),
        200,
        inv_json.clone(),
    )
    .await;
    mount_json_endpoint(
        mock_server,
        (
            "PUT",
            "/v1/invoices/00000000-0000-0000-0000-000000000001/cancel",
        ),
        200,
        inv_json,
    )
    .await;
    let auth_json = json!({
        "id": "ca_mock_01",
        "status": "initiated",
        "transaction_url": "https://api.moyasar.com/v1/card_auths/ca_mock_01/form"
    });
    mount_json_endpoint(
        mock_server,
        ("POST", "/v1/card_auths"),
        201,
        auth_json.clone(),
    )
    .await;
    mount_json_endpoint(
        mock_server,
        ("GET", "/v1/card_auths/ca_mock_01"),
        200,
        auth_json,
    )
    .await;
}

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
        .respond_with(ResponseTemplate::new(201).set_body_json(payment_fixture(
            "pay_mock_01",
            "paid",
            0,
        )))
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

/// Verifies `PaymentsService::list`, `RefundsService::create`, and `CustomersService::charge`.
#[actix_rt::test]
async fn list_and_refund_payments_and_charge_customer_succeed() {
    let (mock_server, app_state) = build_test_harness().await;
    mount_extended_payment_mocks(&mock_server).await;
    app_state
        .vault
        .associate("cus_actix_1", TokenId::new("token_mock_01").expect("valid"))
        .await
        .expect("associate token");

    let app = test::init_service(
        App::new()
            .app_data(app_state)
            .configure(configure_test_routes),
    )
    .await;

    let list_req = test::TestRequest::get().uri("/v1/payments").to_request();
    let list_resp = test::call_service(&app, list_req).await;
    assert_eq!(list_resp.status(), StatusCode::OK);

    let refund_req = test::TestRequest::post()
        .uri("/v1/payments/pay_mock_01/refund")
        .set_json(json!({ "amount": 15000 }))
        .to_request();
    let refund_resp = test::call_service(&app, refund_req).await;
    assert_eq!(refund_resp.status(), StatusCode::OK);

    let charge_req = test::TestRequest::post()
        .uri("/v1/customers/cus_actix_1/charge")
        .set_json(json!({ "amount": 15000, "currency": "SAR" }))
        .to_request();
    let charge_resp = test::call_service(&app, charge_req).await;
    assert_eq!(charge_resp.status(), StatusCode::CREATED);
}

/// Verifies `TokensService::create`, `TokensService::retrieve`, and `TokensService::delete`.
#[actix_rt::test]
async fn tokens_create_retrieve_and_delete_lifecycle_succeeds() {
    let (mock_server, app_state) = build_test_harness().await;
    mount_extended_payment_mocks(&mock_server).await;

    let app = test::init_service(
        App::new()
            .app_data(app_state)
            .configure(configure_test_routes),
    )
    .await;

    let create_req = test::TestRequest::post()
        .uri("/v1/tokens")
        .set_json(json!({
            "name": "Amina Ali",
            "number": "4111111111111111",
            "month": "08",
            "year": "2029",
            "cvc": "123"
        }))
        .to_request();
    let create_resp = test::call_service(&app, create_req).await;
    assert_eq!(create_resp.status(), StatusCode::CREATED);

    let get_req = test::TestRequest::get()
        .uri("/v1/tokens/token_mock_01")
        .to_request();
    let get_resp = test::call_service(&app, get_req).await;
    assert_eq!(get_resp.status(), StatusCode::OK);

    let del_req = test::TestRequest::delete()
        .uri("/v1/tokens/token_mock_01")
        .to_request();
    let del_resp = test::call_service(&app, del_req).await;
    assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);
}

/// Verifies `PaymentIntentsService` create, retrieve, fulfill, and cancel routes.
#[actix_rt::test]
async fn payment_intents_create_retrieve_fulfill_and_cancel_succeed() {
    let (mock_server, app_state) = build_test_harness().await;
    mount_intent_invoice_and_auth_mocks(&mock_server).await;

    let app = test::init_service(
        App::new()
            .app_data(app_state)
            .configure(configure_test_routes),
    )
    .await;

    let create_req = test::TestRequest::post()
        .uri("/v1/payment_intents")
        .set_json(json!({
            "amount": 15000,
            "currency": "SAR",
            "callback_url": "https://merchant.example/cb"
        }))
        .to_request();
    let create_resp = test::call_service(&app, create_req).await;
    assert_eq!(create_resp.status(), StatusCode::CREATED);

    let get_req = test::TestRequest::get()
        .uri("/v1/payment_intents/pi_mock_01")
        .to_request();
    let get_resp = test::call_service(&app, get_req).await;
    assert_eq!(get_resp.status(), StatusCode::OK);

    let fulfill_req = test::TestRequest::post()
        .uri("/v1/payment_intents/pi_mock_01/fulfill")
        .set_json(json!({
            "client_secret": "csec_actix_secret_123456",
            "token_id": "token_mock_01"
        }))
        .to_request();
    let fulfill_resp = test::call_service(&app, fulfill_req).await;
    assert_eq!(fulfill_resp.status(), StatusCode::OK);

    let cancel_req = test::TestRequest::post()
        .uri("/v1/payment_intents/pi_mock_01/cancel")
        .to_request();
    let cancel_resp = test::call_service(&app, cancel_req).await;
    assert_eq!(cancel_resp.status(), StatusCode::OK);
}

/// Verifies `InvoicesService` and `CardAuthsService` routes end-to-end.
#[actix_rt::test]
async fn invoices_and_card_auths_routes_succeed() {
    let (mock_server, app_state) = build_test_harness().await;
    mount_intent_invoice_and_auth_mocks(&mock_server).await;

    let app = test::init_service(
        App::new()
            .app_data(app_state)
            .configure(configure_test_routes),
    )
    .await;

    let inv_req = test::TestRequest::post()
        .uri("/v1/invoices")
        .set_json(json!({ "amount": 50000, "currency": "SAR" }))
        .to_request();
    let inv_resp = test::call_service(&app, inv_req).await;
    assert_eq!(inv_resp.status(), StatusCode::CREATED);

    let cancel_inv = test::TestRequest::put()
        .uri("/v1/invoices/00000000-0000-0000-0000-000000000001/cancel")
        .to_request();
    let cancel_resp = test::call_service(&app, cancel_inv).await;
    assert_eq!(cancel_resp.status(), StatusCode::OK);

    let auth_req = test::TestRequest::post()
        .uri("/v1/card_auths")
        .set_json(json!({
            "amount": 15000,
            "currency": "SAR",
            "callback_url": "https://merchant.example/3ds",
            "name": "Amina Ali",
            "number": "4111111111111111",
            "cvc": "123",
            "month": 8,
            "year": 2029
        }))
        .to_request();
    let auth_resp = test::call_service(&app, auth_req).await;
    assert_eq!(auth_resp.status(), StatusCode::CREATED);

    let get_auth = test::TestRequest::get()
        .uri("/v1/card_auths/ca_mock_01")
        .to_request();
    let get_auth_resp = test::call_service(&app, get_auth).await;
    assert_eq!(get_auth_resp.status(), StatusCode::OK);
}
