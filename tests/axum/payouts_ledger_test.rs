//! E2E tests for `PayoutAccountsService`, `PayoutsService`, `SettlementsService`,
//! `TransfersService`, and `InternalTransactionsService`.
#![forbid(unsafe_code)]

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use some_moyasar::test_utils::fixtures::FIXTURE_TIMESTAMP;
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::spawn_test_app;

/// Sends an HTTP request through the Axum router via `tower::ServiceExt::oneshot` and parses JSON.
async fn dispatch_json(
    app: Router,
    req_method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(req_method).uri(uri);
    let payload = match body {
        Some(value) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            let bytes = serde_json::to_vec(&value).expect("serialize request json");
            Body::from(bytes)
        }
        None => Body::empty(),
    };
    let req = builder.body(payload).expect("build HTTP request");
    let resp = app.oneshot(req).await.expect("oneshot request succeeds");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read response body");
    let json = serde_json::from_slice(&bytes).expect("parse response json");
    (status, json)
}

/// Builds a deterministic pagination metadata JSON object.
fn meta_json() -> Value {
    json!({
        "current_page": 1,
        "next_page": null,
        "prev_page": null,
        "total_pages": 1,
        "total_count": 1
    })
}

/// Builds a deterministic Moyasar `PayoutAccount` JSON fixture.
fn payout_account_json(id: &str) -> Value {
    json!({
        "id": id,
        "account_type": "bank",
        "currency": "SAR",
        "properties": {
            "iban": "SA0380000000608010167519",
            "name": "Acme Trading LLC",
            "bank_name": "Al Rajhi Bank"
        },
        "created_at": FIXTURE_TIMESTAMP
    })
}

/// Builds a deterministic Moyasar `Payout` JSON fixture.
fn payout_json(id: &str, status: &str) -> Value {
    json!({
        "id": id,
        "channel": "sarie",
        "status": status,
        "amount": 50000,
        "message": "Disbursement processed",
        "failure_reason": null
    })
}

/// Builds a deterministic Moyasar `Settlement` JSON fixture.
fn settlement_json(id: &str) -> Value {
    json!({
        "id": id,
        "recipient_type": "entity",
        "currency": "SAR",
        "source_currency": "SAR",
        "amount": 100000,
        "fee": 2000,
        "tax": 300,
        "reference": "SET-REF-01",
        "settlement_count": 4,
        "created_at": FIXTURE_TIMESTAMP
    })
}

/// Builds a deterministic Moyasar `Transfer` JSON fixture.
fn transfer_json(id: &str) -> Value {
    json!({
        "id": id,
        "recipient_type": "entity",
        "recipient_id": "550e8400-e29b-41d4-a716-446655440000",
        "amount": 75000,
        "fee": 1500,
        "tax": 225,
        "currency": "SAR",
        "reference": "TR-REF-01",
        "created_at": FIXTURE_TIMESTAMP
    })
}

/// Builds a deterministic Moyasar `InternalTransaction` JSON fixture.
fn internal_transaction_json(id: &str) -> Value {
    json!({
        "id": id,
        "recipient_type": "entity",
        "recipient_id": "550e8400-e29b-41d4-a716-446655440000",
        "amount": 45000,
        "currency": "SAR",
        "transfer_id": "tr_mock_01",
        "settled_at": FIXTURE_TIMESTAMP,
        "created_at": FIXTURE_TIMESTAMP,
        "updated_at": FIXTURE_TIMESTAMP
    })
}

/// Verifies `POST /payout_accounts` (`201 Created`) and `GET /payout_accounts` (`200 OK`).
#[tokio::test]
async fn e2e_create_and_list_payout_accounts() {
    let (mock, router) = spawn_test_app().await;
    let account = payout_account_json("pa_mock_01");
    let list_payload = json!({
        "payout_accounts": [account.clone()],
        "meta": meta_json()
    });
    Mock::given(method("POST"))
        .and(path("/v1/payout_accounts"))
        .respond_with(ResponseTemplate::new(201).set_body_json(account))
        .expect(1)
        .mount(mock.server())
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/payout_accounts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(list_payload))
        .expect(1)
        .mount(mock.server())
        .await;
    let create_req = json!({
        "account_type": "bank",
        "properties": {
            "iban": "SA0380000000608010167519",
            "name": "Acme Trading LLC",
            "bank_name": "Al Rajhi Bank"
        },
        "credentials": {
            "client_id": "cid_test",
            "client_secret": "csec_test"
        }
    });
    let (create_status, created) = dispatch_json(
        router.clone(),
        Method::POST,
        "/payout_accounts",
        Some(create_req),
    )
    .await;
    let (list_status, listed) =
        dispatch_json(router, Method::GET, "/payout_accounts", None).await;
    assert_eq!(create_status, StatusCode::CREATED);
    assert_eq!(created["id"], "pa_mock_01");
    assert_eq!(created["account_type"], "bank");
    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(listed["items"][0]["id"], "pa_mock_01");
    assert_eq!(listed["meta"]["total_count"], 1);
}

/// Verifies `POST /payout_accounts` rejects malformed Saudi IBANs before upstream dispatch.
#[tokio::test]
async fn e2e_create_payout_account_rejects_invalid_iban() {
    let (_mock, router) = spawn_test_app().await;
    let bad_req = json!({
        "account_type": "bank",
        "properties": {
            "iban": "SA00INVALID",
            "name": "Acme Trading LLC"
        },
        "credentials": "secret_cred"
    });
    let (status, body) =
        dispatch_json(router, Method::POST, "/payout_accounts", Some(bad_req)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "validation_error");
}

/// Verifies `POST /payouts` (`201 Created`) and `GET /payouts/{id}` (`200 OK`).
#[tokio::test]
async fn e2e_create_and_retrieve_payout() {
    let (mock, router) = spawn_test_app().await;
    let queued = payout_json("pout_mock_01", "queued");
    let paid = payout_json("pout_mock_01", "paid");
    Mock::given(method("POST"))
        .and(path("/v1/payouts"))
        .respond_with(ResponseTemplate::new(201).set_body_json(queued))
        .expect(1)
        .mount(mock.server())
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/payouts/pout_mock_01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(paid))
        .expect(1)
        .mount(mock.server())
        .await;
    let create_req = json!({
        "source_id": "pa_mock_01",
        "sequence_number": "1234567890123456",
        "amount": 50000,
        "purpose": "payment_to_merchant",
        "destination": {
            "type": "bank",
            "iban": "SA0380000000608010167519",
            "name": "Vendor Co",
            "mobile": "0515555555",
            "country": "SA",
            "city": "Riyadh"
        },
        "comment": "Supplier settlement"
    });
    let (create_status, created) =
        dispatch_json(router.clone(), Method::POST, "/payouts", Some(create_req)).await;
    let (get_status, fetched) =
        dispatch_json(router, Method::GET, "/payouts/pout_mock_01", None).await;
    assert_eq!(create_status, StatusCode::CREATED);
    assert_eq!(created["id"], "pout_mock_01");
    assert_eq!(created["status"], "queued");
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["id"], "pout_mock_01");
    assert_eq!(fetched["status"], "paid");
}

/// Verifies `GET /settlements/balance` (`200 OK`) and `GET /settlements/{id}` (`200 OK`).
#[tokio::test]
async fn e2e_fetch_settlement_balance_and_retrieve_settlement() {
    let (mock, router) = spawn_test_app().await;
    let balance = json!({
        "currency": "SAR",
        "available": 120000,
        "pending": 30000,
        "reserved": 5000,
        "available_format": "1,200.00 SAR",
        "pending_format": "300.00 SAR",
        "reserved_format": "50.00 SAR"
    });
    let settlement = settlement_json("set_mock_01");
    Mock::given(method("GET"))
        .and(path("/v1/settlements/balance"))
        .respond_with(ResponseTemplate::new(200).set_body_json(balance))
        .expect(1)
        .mount(mock.server())
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/settlements/set_mock_01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(settlement))
        .expect(1)
        .mount(mock.server())
        .await;
    let (bal_status, bal_body) =
        dispatch_json(router.clone(), Method::GET, "/settlements/balance", None).await;
    let (set_status, set_body) =
        dispatch_json(router, Method::GET, "/settlements/set_mock_01", None).await;
    assert_eq!(bal_status, StatusCode::OK);
    assert_eq!(bal_body["currency"], "SAR");
    assert_eq!(bal_body["available"], 120000);
    assert_eq!(set_status, StatusCode::OK);
    assert_eq!(set_body["id"], "set_mock_01");
    assert_eq!(set_body["amount"], 100000);
}

/// Verifies `GET /transfers` (`200 OK`) and `GET /transfers/{id}` (`200 OK`).
#[tokio::test]
async fn e2e_list_and_retrieve_transfers() {
    let (mock, router) = spawn_test_app().await;
    let transfer = transfer_json("tr_mock_01");
    let list_payload = json!({
        "transfers": [transfer.clone()],
        "meta": meta_json()
    });
    Mock::given(method("GET"))
        .and(path("/v1/transfers"))
        .respond_with(ResponseTemplate::new(200).set_body_json(list_payload))
        .expect(1)
        .mount(mock.server())
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/transfers/tr_mock_01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(transfer))
        .expect(1)
        .mount(mock.server())
        .await;
    let (list_status, listed) =
        dispatch_json(router.clone(), Method::GET, "/transfers", None).await;
    let (get_status, fetched) =
        dispatch_json(router, Method::GET, "/transfers/tr_mock_01", None).await;
    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(listed["items"][0]["id"], "tr_mock_01");
    assert_eq!(listed["meta"]["total_count"], 1);
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(fetched["id"], "tr_mock_01");
    assert_eq!(fetched["amount"], 75000);
}

/// Verifies `GET /internal_transactions` (`200 OK`) lists ledger internal transactions.
#[tokio::test]
async fn e2e_list_internal_transactions() {
    let (mock, router) = spawn_test_app().await;
    let txn = internal_transaction_json("itx_mock_01");
    let list_payload = json!({
        "internal_transactions": [txn],
        "meta": meta_json()
    });
    Mock::given(method("GET"))
        .and(path("/v1/internal_transactions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(list_payload))
        .expect(1)
        .mount(mock.server())
        .await;
    let (status, body) =
        dispatch_json(router, Method::GET, "/internal_transactions", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"][0]["id"], "itx_mock_01");
    assert_eq!(body["items"][0]["amount"], 45000);
    assert_eq!(body["items"][0]["transfer_id"], "tr_mock_01");
    assert_eq!(body["meta"]["current_page"], 1);
}
