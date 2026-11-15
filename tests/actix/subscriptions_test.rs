//! E2E tests for subscriptions, payouts, settlements, sources, Apple Pay, and unified gateway.

use actix_web::http::StatusCode;
use actix_web::{App, test};
use serde_json::{Value, json};
use some_moyasar::prelude::derive_subscription_given_id;
use wiremock::matchers::{body_json, body_partial_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{mount_json_endpoint, paginated_envelope};

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

/// Mounts payout account and payout disbursement mocks on `mock_server`.
async fn mount_payout_mocks(mock_server: &some_moyasar::test_utils::MoyasarMockServer) {
    let account_json = json!({
        "id": "pa_mock_01",
        "account_type": "bank",
        "currency": "SAR",
        "properties": {
            "iban": "SA0380000000608010167519",
            "name": "Some CS LLC"
        },
        "created_at": MOCK_CREATED_AT
    });
    mount_json_endpoint(
        mock_server,
        ("POST", "/v1/payout_accounts"),
        201,
        account_json.clone(),
    )
    .await;
    mount_json_endpoint(
        mock_server,
        ("GET", "/v1/payout_accounts"),
        200,
        paginated_envelope(json!([account_json])),
    )
    .await;
    let payout_json = json!({
        "id": "po_mock_01",
        "channel": "sarie",
        "status": "initiated",
        "amount": 25000,
        "message": "Queued",
        "failure_reason": null
    });
    mount_json_endpoint(
        mock_server,
        ("POST", "/v1/payouts"),
        201,
        payout_json.clone(),
    )
    .await;
    mount_json_endpoint(
        mock_server,
        ("GET", "/v1/payouts/po_mock_01"),
        200,
        payout_json,
    )
    .await;
}

/// Mounts settlement, balance, transfer, internal transaction, and source mocks on `mock_server`.
async fn mount_ledger_and_source_mocks(
    mock_server: &some_moyasar::test_utils::MoyasarMockServer,
) {
    mount_json_endpoint(
        mock_server,
        ("GET", "/v1/settlements"),
        200,
        paginated_envelope(json!([{
            "id": "set_mock_01",
            "recipient_type": "entity",
            "currency": "SAR",
            "source_currency": "SAR",
            "amount": 100000,
            "fee": 1500,
            "tax": 225,
            "created_at": MOCK_CREATED_AT
        }])),
    )
    .await;
    mount_json_endpoint(
        mock_server,
        ("GET", "/v1/settlements/balance"),
        200,
        json!({
            "currency": "SAR",
            "available": 85000,
            "pending": 15000,
            "reserved": 0
        }),
    )
    .await;
    mount_json_endpoint(
        mock_server,
        ("GET", "/v1/transfers"),
        200,
        paginated_envelope(json!([{
            "id": "tr_mock_01",
            "amount": 50000,
            "currency": "SAR",
            "created_at": MOCK_CREATED_AT
        }])),
    )
    .await;
    mount_json_endpoint(
        mock_server,
        ("GET", "/v1/internal_transactions"),
        200,
        paginated_envelope(json!([{
            "id": "itx_mock_01",
            "recipient_type": "entity",
            "recipient_id": "00000000-0000-0000-0000-000000000009",
            "amount": 25000,
            "currency": "SAR",
            "created_at": MOCK_CREATED_AT,
            "updated_at": MOCK_CREATED_AT
        }])),
    )
    .await;
    mount_json_endpoint(
        mock_server,
        ("POST", "/v1/source/issuer"),
        200,
        json!({
            "issuer_name": "Al Rajhi Bank",
            "issuer_country": "SA",
            "issuer_card_type": "debit",
            "first_digits": "411111"
        }),
    )
    .await;
    mount_json_endpoint(
        mock_server,
        ("POST", "/v1/applepay/initiate"),
        200,
        json!({
            "epochTimestamp": 1700000000,
            "expiresAt": 1700003600,
            "merchantSessionIdentifier": "msess_actix_01",
            "nonce": "nonce_01",
            "merchantIdentifier": "merchant.tech.someofcs",
            "domainName": "store.someofcs.tech",
            "displayName": "Some(CS)",
            "signature": "sig_mock_01"
        }),
    )
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
    assert_eq!(resp.status(), StatusCode::CREATED);

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
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["amount_reversed"], 19934);
}

/// Verifies `PayoutAccountsService` and `PayoutsService` routes end-to-end.
#[actix_rt::test]
async fn payout_accounts_and_payouts_routes_succeed() {
    let (mock_server, app_state) = super::build_test_harness().await;
    mount_payout_mocks(&mock_server).await;

    let app = test::init_service(
        App::new()
            .app_data(app_state)
            .configure(super::configure_test_routes),
    )
    .await;

    let acc_req = test::TestRequest::post()
        .uri("/v1/payout_accounts")
        .set_json(json!({
            "iban": "SA0380000000608010167519",
            "name": "Some CS LLC",
            "credentials": "{\"client_id\":\"cid\"}"
        }))
        .to_request();
    let acc_resp = test::call_service(&app, acc_req).await;
    assert_eq!(acc_resp.status(), StatusCode::CREATED);

    let list_acc = test::TestRequest::get()
        .uri("/v1/payout_accounts")
        .to_request();
    let list_resp = test::call_service(&app, list_acc).await;
    assert_eq!(list_resp.status(), StatusCode::OK);

    let pay_req = test::TestRequest::post()
        .uri("/v1/payouts")
        .set_json(json!({
            "source_id": "pa_mock_01",
            "amount": 25000,
            "purpose": "bills_or_rent",
            "iban": "SA0380000000608010167519",
            "name": "Amina Ali",
            "mobile": "+966512345678",
            "country": "SA",
            "city": "Riyadh"
        }))
        .to_request();
    let pay_resp = test::call_service(&app, pay_req).await;
    assert_eq!(pay_resp.status(), StatusCode::CREATED);

    let get_pay = test::TestRequest::get()
        .uri("/v1/payouts/po_mock_01")
        .to_request();
    let get_resp = test::call_service(&app, get_pay).await;
    assert_eq!(get_resp.status(), StatusCode::OK);
}

/// Verifies `SettlementsService`, `TransfersService`, and `InternalTransactionsService` routes.
#[actix_rt::test]
async fn settlements_balance_transfers_and_internal_transactions_succeed() {
    let (mock_server, app_state) = super::build_test_harness().await;
    mount_ledger_and_source_mocks(&mock_server).await;

    let app = test::init_service(
        App::new()
            .app_data(app_state)
            .configure(super::configure_test_routes),
    )
    .await;

    let set_req = test::TestRequest::get().uri("/v1/settlements").to_request();
    let set_resp = test::call_service(&app, set_req).await;
    assert_eq!(set_resp.status(), StatusCode::OK);

    let bal_req = test::TestRequest::get()
        .uri("/v1/settlements/balance")
        .to_request();
    let bal_resp = test::call_service(&app, bal_req).await;
    assert_eq!(bal_resp.status(), StatusCode::OK);

    let tr_req = test::TestRequest::get().uri("/v1/transfers").to_request();
    let tr_resp = test::call_service(&app, tr_req).await;
    assert_eq!(tr_resp.status(), StatusCode::OK);

    let itx_req = test::TestRequest::get()
        .uri("/v1/internal_transactions")
        .to_request();
    let itx_resp = test::call_service(&app, itx_req).await;
    assert_eq!(itx_resp.status(), StatusCode::OK);
}

/// Verifies `SourcesService::get_issuer`, `ApplePayService::initiate_session`, and `UnifiedPaymentGateway`.
#[actix_rt::test]
async fn sources_issuer_apple_pay_and_unified_intent_succeed() {
    let (mock_server, app_state) = super::build_test_harness().await;
    mount_ledger_and_source_mocks(&mock_server).await;

    let app = test::init_service(
        App::new()
            .app_data(app_state)
            .configure(super::configure_test_routes),
    )
    .await;

    let bin_req = test::TestRequest::post()
        .uri("/v1/sources/issuer")
        .set_json(json!({ "first_digits": "411111" }))
        .to_request();
    let bin_resp = test::call_service(&app, bin_req).await;
    assert_eq!(bin_resp.status(), StatusCode::OK);

    let ap_req = test::TestRequest::post()
        .uri("/v1/applepay/initiate")
        .set_json(json!({
            "validation_url": "https://apple-pay-gateway.apple.com/paymentservices/startSession",
            "display_name": "Some(CS)",
            "domain_name": "store.someofcs.tech",
            "publishable_api_key": "pk_test_actix_default"
        }))
        .to_request();
    let ap_resp = test::call_service(&app, ap_req).await;
    assert_eq!(ap_resp.status(), StatusCode::OK);

    let uni_req = test::TestRequest::post()
        .uri("/v1/unified/intents")
        .set_json(json!({
            "amount_minor": 15000,
            "currency": "SAR",
            "callback_url": "https://merchant.example/cb",
            "description": "Unified Actix Order"
        }))
        .to_request();
    let uni_resp = test::call_service(&app, uni_req).await;
    assert_eq!(uni_resp.status(), StatusCode::CREATED);
}
