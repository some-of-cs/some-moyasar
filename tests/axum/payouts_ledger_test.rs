//! E2E tests for `PayoutAccountsService`, `PayoutsService`, `SettlementsService`,
//! `TransfersService`, and `InternalTransactionsService`.
#![forbid(unsafe_code)]

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use some_moyasar::test_utils::MoyasarMockServer;
use some_moyasar::test_utils::fixtures::FIXTURE_TIMESTAMP;
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::spawn_test_app;

/// Paired upstream Moyasar path and Axum application route path.
#[derive(Clone, Copy)]
struct RoutePair {
    /// Upstream Moyasar API path mounted on the wiremock server.
    upstream: &'static str,
    /// Axum router path dispatched by the test client.
    app: &'static str,
}

const PAYOUT_ACCOUNTS_ROUTE: RoutePair = RoutePair {
    upstream: "/v1/payout_accounts",
    app: "/payout_accounts",
};
const PAYOUTS_ROUTE: RoutePair = RoutePair {
    upstream: "/v1/payouts",
    app: "/payouts",
};
const PAYOUT_BY_ID_ROUTE: RoutePair = RoutePair {
    upstream: "/v1/payouts/pout_mock_01",
    app: "/payouts/pout_mock_01",
};
const SETTLEMENT_BALANCE_ROUTE: RoutePair = RoutePair {
    upstream: "/v1/settlements/balance",
    app: "/settlements/balance",
};
const SETTLEMENT_BY_ID_ROUTE: RoutePair = RoutePair {
    upstream: "/v1/settlements/set_mock_01",
    app: "/settlements/set_mock_01",
};
const TRANSFERS_ROUTE: RoutePair = RoutePair {
    upstream: "/v1/transfers",
    app: "/transfers",
};
const TRANSFER_BY_ID_ROUTE: RoutePair = RoutePair {
    upstream: "/v1/transfers/tr_mock_01",
    app: "/transfers/tr_mock_01",
};
const INTERNAL_TXNS_ROUTE: RoutePair = RoutePair {
    upstream: "/v1/internal_transactions",
    app: "/internal_transactions",
};

/// Builds an HTTP [`Request`] with an optional serialized JSON body.
fn build_json_request(req_method: Method, route: RoutePair, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder().method(req_method).uri(route.app);
    let payload = match body {
        Some(value) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(serde_json::to_vec(&value).expect("serialize request json"))
        }
        None => Body::empty(),
    };
    builder.body(payload).expect("build HTTP request")
}

/// Sends an HTTP request through the Axum router, asserts `expected` status, and parses JSON.
async fn dispatch_expect(
    app: Router,
    req_method: Method,
    route: RoutePair,
    body: Option<Value>,
    expected: StatusCode,
) -> Value {
    let req = build_json_request(req_method, route, body);
    let resp = app.oneshot(req).await.expect("oneshot request succeeds");
    assert_eq!(resp.status(), expected);
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read response body");
    serde_json::from_slice(&bytes).expect("parse response json")
}

/// Mounts a single-shot JSON mock expectation on the upstream [`MoyasarMockServer`].
async fn mount_json_mock(
    mock: &MoyasarMockServer,
    req_method: Method,
    route: RoutePair,
    body: Value,
) {
    let is_post = req_method == Method::POST;
    let code = if is_post { 201 } else { 200 };
    Mock::given(method(req_method.as_str()))
        .and(path(route.upstream))
        .respond_with(ResponseTemplate::new(code).set_body_json(body))
        .expect(1)
        .mount(mock.server())
        .await;
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

/// Builds a deterministic Moyasar `PayoutAccount` JSON fixture including an upstream credential.
fn payout_account_json() -> Value {
    json!({
        "id": "pa_mock_01",
        "account_type": "bank",
        "currency": "SAR",
        "properties": {
            "iban": "SA0380000000608010167519",
            "name": "Acme Trading LLC",
            "bank_name": "Al Rajhi Bank"
        },
        "credentials": "secret_upstream_cred",
        "created_at": FIXTURE_TIMESTAMP
    })
}

/// Builds a valid `POST /payout_accounts` request payload.
fn create_payout_account_req_json() -> Value {
    json!({
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
    })
}

/// Builds a deterministic Moyasar `Payout` JSON fixture (`queued` or `paid`).
fn payout_json(is_paid: bool) -> Value {
    let status = if is_paid { "paid" } else { "queued" };
    json!({
        "id": "pout_mock_01",
        "channel": "sarie",
        "status": status,
        "amount": 50000,
        "message": "Disbursement processed",
        "failure_reason": null
    })
}

/// Builds a valid `POST /payouts` request payload.
fn create_payout_req_json() -> Value {
    json!({
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
    })
}

/// Builds a deterministic Moyasar `Settlement` JSON fixture.
fn settlement_json() -> Value {
    json!({
        "id": "set_mock_01",
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

/// Builds a deterministic `Transfer` (`false`) or `InternalTransaction` (`true`) JSON fixture.
fn ledger_record_json(is_internal_txn: bool) -> Value {
    if is_internal_txn {
        return json!({
            "id": "itx_mock_01",
            "recipient_type": "entity",
            "recipient_id": "550e8400-e29b-41d4-a716-446655440000",
            "amount": 45000,
            "currency": "SAR",
            "transfer_id": "tr_mock_01",
            "settled_at": FIXTURE_TIMESTAMP,
            "created_at": FIXTURE_TIMESTAMP,
            "updated_at": FIXTURE_TIMESTAMP
        });
    }
    json!({
        "id": "tr_mock_01",
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

/// Verifies `POST /payout_accounts` (`201 Created`) and `GET /payout_accounts` (`200 OK`).
#[tokio::test]
async fn e2e_create_and_list_payout_accounts() {
    let (mock, router) = spawn_test_app().await;
    let account = payout_account_json();
    let list_payload = json!({ "payout_accounts": [account.clone()], "meta": meta_json() });
    mount_json_mock(&mock, Method::POST, PAYOUT_ACCOUNTS_ROUTE, account).await;
    mount_json_mock(&mock, Method::GET, PAYOUT_ACCOUNTS_ROUTE, list_payload).await;
    let create_req = create_payout_account_req_json();
    let created = dispatch_expect(
        router.clone(),
        Method::POST,
        PAYOUT_ACCOUNTS_ROUTE,
        Some(create_req),
        StatusCode::CREATED,
    )
    .await;
    let listed = dispatch_expect(
        router,
        Method::GET,
        PAYOUT_ACCOUNTS_ROUTE,
        None,
        StatusCode::OK,
    )
    .await;
    let omits_credentials = created.get("credentials").is_none()
        && listed["items"][0].get("credentials").is_none()
        && !created.to_string().contains("secret_upstream_cred")
        && !listed.to_string().contains("secret_upstream_cred");
    assert_eq!(created["id"], "pa_mock_01");
    assert_eq!(listed["items"][0]["id"], "pa_mock_01");
    assert!(omits_credentials);
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
    let body = dispatch_expect(
        router,
        Method::POST,
        PAYOUT_ACCOUNTS_ROUTE,
        Some(bad_req),
        StatusCode::BAD_REQUEST,
    )
    .await;
    assert_eq!(body["error"]["code"], "validation_error");
}

/// Verifies `POST /payouts` (`201 Created`) and `GET /payouts/{id}` (`200 OK`).
#[tokio::test]
async fn e2e_create_and_retrieve_payout() {
    let (mock, router) = spawn_test_app().await;
    mount_json_mock(&mock, Method::POST, PAYOUTS_ROUTE, payout_json(false)).await;
    mount_json_mock(&mock, Method::GET, PAYOUT_BY_ID_ROUTE, payout_json(true)).await;
    let create_req = create_payout_req_json();
    let created = dispatch_expect(
        router.clone(),
        Method::POST,
        PAYOUTS_ROUTE,
        Some(create_req),
        StatusCode::CREATED,
    )
    .await;
    let fetched = dispatch_expect(
        router,
        Method::GET,
        PAYOUT_BY_ID_ROUTE,
        None,
        StatusCode::OK,
    )
    .await;
    assert_eq!(created["id"], "pout_mock_01");
    assert_eq!(created["status"], "queued");
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
    mount_json_mock(&mock, Method::GET, SETTLEMENT_BALANCE_ROUTE, balance).await;
    mount_json_mock(&mock, Method::GET, SETTLEMENT_BY_ID_ROUTE, settlement_json()).await;
    let bal_body = dispatch_expect(
        router.clone(),
        Method::GET,
        SETTLEMENT_BALANCE_ROUTE,
        None,
        StatusCode::OK,
    )
    .await;
    let set_body = dispatch_expect(
        router,
        Method::GET,
        SETTLEMENT_BY_ID_ROUTE,
        None,
        StatusCode::OK,
    )
    .await;
    assert_eq!(bal_body["available"], 120000);
    assert_eq!(set_body["id"], "set_mock_01");
    assert_eq!(set_body["amount"], 100000);
}

/// Verifies `GET /transfers` (`200 OK`) and `GET /transfers/{id}` (`200 OK`).
#[tokio::test]
async fn e2e_list_and_retrieve_transfers() {
    let (mock, router) = spawn_test_app().await;
    let transfer = ledger_record_json(false);
    let list_payload = json!({ "transfers": [transfer.clone()], "meta": meta_json() });
    mount_json_mock(&mock, Method::GET, TRANSFERS_ROUTE, list_payload).await;
    mount_json_mock(&mock, Method::GET, TRANSFER_BY_ID_ROUTE, transfer).await;
    let listed = dispatch_expect(
        router.clone(),
        Method::GET,
        TRANSFERS_ROUTE,
        None,
        StatusCode::OK,
    )
    .await;
    let fetched = dispatch_expect(
        router,
        Method::GET,
        TRANSFER_BY_ID_ROUTE,
        None,
        StatusCode::OK,
    )
    .await;
    assert_eq!(listed["items"][0]["id"], "tr_mock_01");
    assert_eq!(listed["meta"]["total_count"], 1);
    assert_eq!(fetched["amount"], 75000);
}

/// Verifies `GET /internal_transactions` (`200 OK`) lists ledger internal transactions.
#[tokio::test]
async fn e2e_list_internal_transactions() {
    let (mock, router) = spawn_test_app().await;
    let txn = ledger_record_json(true);
    let list_payload = json!({ "internal_transactions": [txn], "meta": meta_json() });
    mount_json_mock(&mock, Method::GET, INTERNAL_TXNS_ROUTE, list_payload).await;
    let body = dispatch_expect(
        router,
        Method::GET,
        INTERNAL_TXNS_ROUTE,
        None,
        StatusCode::OK,
    )
    .await;
    assert_eq!(body["items"][0]["id"], "itx_mock_01");
    assert_eq!(body["items"][0]["amount"], 45000);
    assert_eq!(body["items"][0]["transfer_id"], "tr_mock_01");
}
