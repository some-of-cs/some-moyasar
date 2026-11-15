//! End-to-end tests for Axum payment intent routes and expiry guard.
#![forbid(unsafe_code)]

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use some_moyasar::resources::payment_intents::models::{
    INTENT_EXPIRY_SECS, PaymentIntent, PaymentIntentStatus,
};
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

/// Maximum response body size read during E2E assertions (64 KiB).
const MAX_BODY_BYTES: usize = 64 * 1024;

/// Specification for mounting a single-use upstream mock and dispatching an Axum request.
struct MockDispatchSpec<'a> {
    http_method: Method,
    upstream_path: &'a str,
    upstream_status: u16,
    mock_body: Value,
    app_uri: &'a str,
    req_body: Option<Value>,
}

/// Builds a valid mock `PaymentIntent` JSON payload matching `models::PaymentIntent`.
fn intent_payload(
    id: &str,
    status: PaymentIntentStatus,
    amount: u64,
    expires_at: DateTime<Utc>,
) -> Value {
    let created_at = expires_at - Duration::seconds(INTENT_EXPIRY_SECS);
    json!({
        "id": id,
        "status": status,
        "amount": amount,
        "amount_format": "250.00 SAR",
        "currency": "SAR",
        "description": "Order #42",
        "callback_url": "https://example.com/callback",
        "client_secret": "cs_test_e2e_secret",
        "expires_at": expires_at.to_rfc3339(),
        "created_at": created_at.to_rfc3339(),
        "updated_at": created_at.to_rfc3339()
    })
}

/// Builds a JSON HTTP request for `oneshot` dispatch against the Axum router.
fn json_request(method: Method, uri: &str, body: Option<Value>) -> Request<Body> {
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

/// Mounts a single-use upstream mock and dispatches a JSON request to the Axum router.
async fn mount_and_dispatch(spec: MockDispatchSpec<'_>) -> axum::response::Response {
    let (mock_server, app) = super::spawn_test_app().await;
    Mock::given(method(spec.http_method.as_str()))
        .and(path(spec.upstream_path))
        .respond_with(ResponseTemplate::new(spec.upstream_status).set_body_json(spec.mock_body))
        .with_priority(1)
        .expect(1)
        .mount(mock_server.server())
        .await;

    let request = json_request(spec.http_method, spec.app_uri, spec.req_body);
    app.oneshot(request).await.expect("router response")
}

/// Asserts a `200 OK` response and returns the parsed JSON body after checking `id` and `status`.
async fn assert_intent_ok(
    response: axum::response::Response,
    expected_id: &str,
    expected_status: PaymentIntentStatus,
) -> Value {
    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json_body(response).await;
    assert_eq!(body["id"], expected_id);
    assert_eq!(body["status"], json!(expected_status));
    body
}

/// Dispatches `GET /intents/{id}` with a fixture at the given expiry offset and verifies usability.
async fn dispatch_get_intent(
    id: &str,
    expiry_offset: Duration,
    expected_usable: bool,
) -> axum::response::Response {
    let expires_at = Utc::now() + expiry_offset;
    let mock_body = intent_payload(id, PaymentIntentStatus::Created, 25000, expires_at);
    let model: PaymentIntent =
        serde_json::from_value(mock_body.clone()).expect("valid PaymentIntent fixture");
    assert_eq!(model.is_usable(), expected_usable);

    let upstream_path = format!("/v1/payment_intents/{id}");
    let app_uri = format!("/intents/{id}");
    mount_and_dispatch(MockDispatchSpec {
        http_method: Method::GET,
        upstream_path: &upstream_path,
        upstream_status: 200,
        mock_body,
        app_uri: &app_uri,
        req_body: None,
    })
    .await
}

/// Verifies `POST /intents` creates a payment intent with integer minor-unit `amount`.
#[tokio::test]
async fn creates_payment_intent_with_integer_minor_unit_amount() {
    let expires_at = Utc::now() + Duration::seconds(INTENT_EXPIRY_SECS);
    let mock_body = intent_payload(
        "pi_e2e_create_01",
        PaymentIntentStatus::Created,
        25000,
        expires_at,
    );
    let req_body = json!({
        "amount": 25000_u64,
        "currency": "SAR",
        "description": "Order #42",
        "callback_url": "https://example.com/callback"
    });

    let response = mount_and_dispatch(MockDispatchSpec {
        http_method: Method::POST,
        upstream_path: "/v1/payment_intents",
        upstream_status: 201,
        mock_body,
        app_uri: "/intents",
        req_body: Some(req_body),
    })
    .await;

    assert!(
        response.status() == StatusCode::CREATED || response.status() == StatusCode::OK,
        "unexpected status: {}",
        response.status()
    );
    let body = read_json_body(response).await;
    assert_eq!(body["id"], "pi_e2e_create_01");
    assert_eq!(body["status"], "created");
    assert_eq!(body["amount"], 25000);
}

/// Verifies `GET /intents/{id}` returns `200 OK` when the intent is still usable.
#[tokio::test]
async fn retrieves_usable_payment_intent_with_200_ok() {
    let response = dispatch_get_intent("pi_e2e_usable_01", Duration::minutes(45), true).await;
    let body = assert_intent_ok(response, "pi_e2e_usable_01", PaymentIntentStatus::Created).await;
    assert_eq!(body["amount"], 25000);
}

/// Verifies `GET /intents/{id}` returns `410 Gone` when `is_usable() == false` due to expiry.
#[tokio::test]
async fn rejects_expired_payment_intent_with_410_gone() {
    let response = dispatch_get_intent("pi_e2e_expired_01", -Duration::minutes(10), false).await;
    assert_eq!(response.status(), StatusCode::GONE);
}

/// Verifies `POST /intents/{id}/cancel` cancels an active payment intent.
#[tokio::test]
async fn cancels_payment_intent_with_200_ok() {
    let expires_at = Utc::now() + Duration::minutes(30);
    let mock_body = intent_payload(
        "pi_e2e_cancel_01",
        PaymentIntentStatus::Canceled,
        25000,
        expires_at,
    );

    let response = mount_and_dispatch(MockDispatchSpec {
        http_method: Method::POST,
        upstream_path: "/v1/payment_intents/pi_e2e_cancel_01/cancel",
        upstream_status: 200,
        mock_body,
        app_uri: "/intents/pi_e2e_cancel_01/cancel",
        req_body: None,
    })
    .await;

    assert_intent_ok(response, "pi_e2e_cancel_01", PaymentIntentStatus::Canceled).await;
}
