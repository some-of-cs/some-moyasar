//! End-to-end tests for Axum token, standalone 3DS card authentication, and invoice routes.
#![forbid(unsafe_code)]

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use some_moyasar::test_utils::fixtures::FIXTURE_TIMESTAMP;
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

/// Maximum response body size read during E2E assertions (64 KiB).
const MAX_BODY_BYTES: usize = 64 * 1024;

/// Deterministic UUID used across invoice E2E route tests.
const TEST_INVOICE_ID: &str = "550e8400-e29b-41d4-a716-446655440000";

/// Specification for mounting a single-use upstream mock and dispatching an Axum request.
struct MockDispatchSpec<'a> {
    /// HTTP method matched by the upstream mock server and sent to the Axum router.
    http_method: Method,
    /// Upstream Moyasar API path expected by the mock server.
    upstream_path: &'a str,
    /// HTTP status code returned by the upstream mock response template.
    upstream_status: u16,
    /// Optional JSON response payload returned by the upstream mock server.
    mock_body: Option<Value>,
    /// Axum router URI path targeted by the test request.
    app_uri: &'a str,
    /// Optional JSON request payload sent to the Axum route handler.
    req_body: Option<Value>,
}

impl<'a> MockDispatchSpec<'a> {
    /// Builds a `201 Created` `POST` dispatch specification with JSON request and mock payloads.
    fn post_json(routes: (&'a str, &'a str), req_body: Value, mock_body: Value) -> Self {
        Self {
            http_method: Method::POST,
            upstream_path: routes.0,
            upstream_status: 201,
            mock_body: Some(mock_body),
            app_uri: routes.1,
            req_body: Some(req_body),
        }
    }

    /// Builds a `200 OK` bodyless request dispatch specification with a JSON mock payload.
    fn without_request_body(
        http_method: Method,
        routes: (&'a str, &'a str),
        mock_body: Value,
    ) -> Self {
        Self {
            http_method,
            upstream_path: routes.0,
            upstream_status: 200,
            mock_body: Some(mock_body),
            app_uri: routes.1,
            req_body: None,
        }
    }
}

/// Builds a valid Moyasar `Token` JSON payload.
fn token_fixture(id: &str, status: &str) -> Value {
    json!({
        "id": id,
        "status": status,
        "number": "4111XXXXXXXX1111",
        "company": "visa",
        "gateway_id": "gw_token_01",
        "created_at": FIXTURE_TIMESTAMP
    })
}

/// Builds a valid Moyasar `CardAuthentication` JSON payload.
fn card_auth_fixture(id: &str, status: &str) -> Value {
    json!({
        "id": id,
        "status": status,
        "transaction_url": "https://acs.moyasar.example/challenge/ca_mock_01",
        "result": {
            "eci": "05",
            "authentication_value": "cavv_test_value",
            "ds_transaction_id": "ds_tx_01",
            "version": "2.2.0",
            "transaction_status": "Y",
            "auth_scheme": "visa",
            "is_frictionless": true
        }
    })
}

/// Builds a valid Moyasar `Invoice` JSON payload.
fn invoice_fixture(id: &str, status: &str) -> Value {
    json!({
        "id": id,
        "status": status,
        "amount": 25000,
        "currency": "SAR",
        "amount_format": "250.00 SAR",
        "description": "Enterprise subscription invoice",
        "url": "https://moyasar.com/invoices/550e8400-e29b-41d4-a716-446655440000",
        "callback_url": "https://merchant.example/invoices/callback",
        "payments": [],
        "created_at": FIXTURE_TIMESTAMP,
        "updated_at": FIXTURE_TIMESTAMP
    })
}

/// Builds an HTTP request for `oneshot` dispatch against the Axum router.
fn build_request(method: Method, uri: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    let payload = match body {
        Some(value) => Body::from(value.to_string()),
        None => Body::empty(),
    };
    builder.body(payload).expect("valid HTTP request")
}

/// Reads and parses a JSON response body from an Axum `Response`.
async fn read_json_body(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), MAX_BODY_BYTES)
        .await
        .expect("readable response body");
    serde_json::from_slice(&bytes).expect("valid JSON response body")
}

/// Mounts a single-use upstream mock and dispatches a request to the Axum router.
async fn mount_and_dispatch(spec: MockDispatchSpec<'_>) -> axum::response::Response {
    let (mock_server, app) = super::spawn_test_app().await;
    let template = match spec.mock_body {
        Some(body) => ResponseTemplate::new(spec.upstream_status).set_body_json(body),
        None => ResponseTemplate::new(spec.upstream_status),
    };
    Mock::given(method(spec.http_method.as_str()))
        .and(path(spec.upstream_path))
        .respond_with(template)
        .with_priority(1)
        .expect(1)
        .mount(mock_server.server())
        .await;

    let request = build_request(spec.http_method, spec.app_uri, spec.req_body);
    app.oneshot(request).await.expect("router response")
}

/// Dispatches a mocked request and asserts the HTTP status plus JSON `"id"` and `"status"` fields.
async fn assert_json_dispatch(spec: MockDispatchSpec<'_>, status: StatusCode) -> Value {
    let expected_id = spec.mock_body.as_ref().expect("mock body")["id"].clone();
    let expected_status = spec.mock_body.as_ref().expect("mock body")["status"].clone();
    let response = mount_and_dispatch(spec).await;
    assert_eq!(response.status(), status);
    let body = read_json_body(response).await;
    assert_eq!(body["id"], expected_id);
    assert_eq!(body["status"], expected_status);
    body
}

/// Verifies `POST /tokens` creates a card token and returns `201 Created`.
#[tokio::test]
async fn e2e_create_token_returns_201_created() {
    let req_body = json!({
        "name": "Maher Mansour",
        "number": "4111111111111111",
        "month": "12",
        "year": "2028",
        "cvc": "123",
        "callback_url": "https://merchant.example/tokens/callback",
        "save_only": true
    });
    let spec = MockDispatchSpec::post_json(
        ("/v1/tokens", "/tokens"),
        req_body,
        token_fixture("token_mock_01", "active"),
    );
    let body = assert_json_dispatch(spec, StatusCode::CREATED).await;
    assert_eq!(body["company"], "visa");
}

/// Verifies `GET /tokens/{id}` retrieves an existing card token and returns `200 OK`.
#[tokio::test]
async fn e2e_retrieve_token_returns_200_ok() {
    let spec = MockDispatchSpec::without_request_body(
        Method::GET,
        ("/v1/tokens/token_mock_01", "/tokens/token_mock_01"),
        token_fixture("token_mock_01", "active"),
    );
    let body = assert_json_dispatch(spec, StatusCode::OK).await;
    assert_eq!(body["number"], "4111XXXXXXXX1111");
}

/// Verifies `DELETE /tokens/{id}` deletes a card token and returns `204 No Content`.
#[tokio::test]
async fn e2e_delete_token_returns_204_no_content() {
    let response = mount_and_dispatch(MockDispatchSpec {
        http_method: Method::DELETE,
        upstream_path: "/v1/tokens/token_mock_01",
        upstream_status: 204,
        mock_body: None,
        app_uri: "/tokens/token_mock_01",
        req_body: None,
    })
    .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

/// Verifies `POST /card_auths` initiates standalone 3DS authentication and returns `201 Created`.
#[tokio::test]
async fn e2e_create_card_auth_returns_201_created() {
    let req_body = json!({
        "amount": 15000,
        "currency": "SAR",
        "callback_url": "https://merchant.example/3ds/callback",
        "name": "Maher Mansour",
        "number": "4111111111111111",
        "cvc": "123",
        "month": 12,
        "year": 2028,
        "description": "Standalone 3DS verification"
    });
    let spec = MockDispatchSpec::post_json(
        ("/v1/card_auths", "/card_auths"),
        req_body,
        card_auth_fixture("ca_mock_01", "initiated"),
    );
    let body = assert_json_dispatch(spec, StatusCode::CREATED).await;
    assert_eq!(body["is_frictionless"], true);
    assert!(body.get("authentication_value").is_none());
}

/// Verifies `GET /card_auths/{id}` retrieves a standalone 3DS record and returns `200 OK`.
#[tokio::test]
async fn e2e_retrieve_card_auth_returns_200_ok() {
    let spec = MockDispatchSpec::without_request_body(
        Method::GET,
        ("/v1/card_auths/ca_mock_01", "/card_auths/ca_mock_01"),
        card_auth_fixture("ca_mock_01", "authenticated"),
    );
    let body = assert_json_dispatch(spec, StatusCode::OK).await;
    assert_eq!(body["is_frictionless"], true);
    assert!(body.get("authentication_value").is_none());
    assert!(body.get("result").is_none());
    assert!(!body.to_string().contains("cavv_test_value"));
}

/// Verifies `POST /invoices` creates a hosted e-invoice and returns `201 Created`.
#[tokio::test]
async fn e2e_create_invoice_returns_201_created() {
    let req_body = json!({
        "amount": 25000,
        "currency": "SAR",
        "description": "Enterprise subscription invoice",
        "callback_url": "https://merchant.example/invoices/callback"
    });
    let spec = MockDispatchSpec::post_json(
        ("/v1/invoices", "/invoices"),
        req_body,
        invoice_fixture(TEST_INVOICE_ID, "created"),
    );
    let body = assert_json_dispatch(spec, StatusCode::CREATED).await;
    assert_eq!(body["amount"], 25000);
}

/// Verifies `GET /invoices/{id}` retrieves a hosted e-invoice by UUID and returns `200 OK`.
#[tokio::test]
async fn e2e_retrieve_invoice_returns_200_ok() {
    let upstream_path = format!("/v1/invoices/{TEST_INVOICE_ID}");
    let app_uri = format!("/invoices/{TEST_INVOICE_ID}");
    let spec = MockDispatchSpec::without_request_body(
        Method::GET,
        (&upstream_path, &app_uri),
        invoice_fixture(TEST_INVOICE_ID, "paid"),
    );
    let body = assert_json_dispatch(spec, StatusCode::OK).await;
    assert_eq!(body["currency"], "SAR");
}

/// Verifies `PUT /invoices/{id}/cancel` cancels an active hosted e-invoice and returns `200 OK`.
#[tokio::test]
async fn e2e_cancel_invoice_returns_200_ok() {
    let upstream_path = format!("/v1/invoices/{TEST_INVOICE_ID}/cancel");
    let app_uri = format!("/invoices/{TEST_INVOICE_ID}/cancel");
    let spec = MockDispatchSpec::without_request_body(
        Method::PUT,
        (&upstream_path, &app_uri),
        invoice_fixture(TEST_INVOICE_ID, "canceled"),
    );
    let body = assert_json_dispatch(spec, StatusCode::OK).await;
    assert_eq!(body["amount_format"], "250.00 SAR");
    assert!(body["payments"].as_array().expect("payments array").is_empty());
}
