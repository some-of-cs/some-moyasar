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

/// Builds a valid mock `PaymentIntent` JSON payload matching `models::PaymentIntent`.
fn intent_payload(id: &str, status: &str, amount: u64, expires_at: DateTime<Utc>) -> Value {
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

/// Verifies `POST /intents` creates a payment intent with integer minor-unit `amount`.
#[tokio::test]
async fn creates_payment_intent_with_integer_minor_unit_amount() {
    let (mock_server, app) = super::spawn_test_app().await;
    let expires_at = Utc::now() + Duration::seconds(INTENT_EXPIRY_SECS);
    let mock_body = intent_payload("pi_e2e_create_01", "created", 25000, expires_at);

    Mock::given(method("POST"))
        .and(path("/v1/payment_intents"))
        .respond_with(ResponseTemplate::new(201).set_body_json(mock_body))
        .with_priority(1)
        .expect(1)
        .mount(mock_server.server())
        .await;

    let req_body = json!({
        "amount": 25000_u64,
        "currency": "SAR",
        "description": "Order #42",
        "callback_url": "https://example.com/callback"
    });
    let request = json_request(Method::POST, "/intents", Some(req_body));
    let response = app.oneshot(request).await.expect("router response");

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
    let (mock_server, app) = super::spawn_test_app().await;
    let expires_at = Utc::now() + Duration::minutes(45);
    let mock_body = intent_payload("pi_e2e_usable_01", "created", 25000, expires_at);

    let model: PaymentIntent =
        serde_json::from_value(mock_body.clone()).expect("valid PaymentIntent fixture");
    assert!(model.is_usable());

    Mock::given(method("GET"))
        .and(path("/v1/payment_intents/pi_e2e_usable_01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mock_body))
        .with_priority(1)
        .expect(1)
        .mount(mock_server.server())
        .await;

    let request = json_request(Method::GET, "/intents/pi_e2e_usable_01", None);
    let response = app.oneshot(request).await.expect("router response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json_body(response).await;
    assert_eq!(body["id"], "pi_e2e_usable_01");
    assert_eq!(body["status"], "created");
    assert_eq!(body["amount"], 25000);
}

/// Verifies `GET /intents/{id}` returns `410 Gone` when `is_usable() == false` due to expiry.
#[tokio::test]
async fn rejects_expired_payment_intent_with_410_gone() {
    let (mock_server, app) = super::spawn_test_app().await;
    let expired_at = Utc::now() - Duration::minutes(10);
    let mock_body = intent_payload("pi_e2e_expired_01", "created", 25000, expired_at);

    let model: PaymentIntent =
        serde_json::from_value(mock_body.clone()).expect("valid PaymentIntent fixture");
    assert_eq!(model.status, PaymentIntentStatus::Created);
    assert!(!model.is_usable());

    Mock::given(method("GET"))
        .and(path("/v1/payment_intents/pi_e2e_expired_01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mock_body))
        .with_priority(1)
        .expect(1)
        .mount(mock_server.server())
        .await;

    let request = json_request(Method::GET, "/intents/pi_e2e_expired_01", None);
    let response = app.oneshot(request).await.expect("router response");

    assert_eq!(response.status(), StatusCode::GONE);
}

/// Verifies `POST /intents/{id}/cancel` cancels an active payment intent.
#[tokio::test]
async fn cancels_payment_intent_with_200_ok() {
    let (mock_server, app) = super::spawn_test_app().await;
    let expires_at = Utc::now() + Duration::minutes(30);
    let mock_body = intent_payload("pi_e2e_cancel_01", "canceled", 25000, expires_at);

    Mock::given(method("POST"))
        .and(path("/v1/payment_intents/pi_e2e_cancel_01/cancel"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mock_body))
        .with_priority(1)
        .expect(1)
        .mount(mock_server.server())
        .await;

    let request = json_request(Method::POST, "/intents/pi_e2e_cancel_01/cancel", None);
    let response = app.oneshot(request).await.expect("router response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json_body(response).await;
    assert_eq!(body["id"], "pi_e2e_cancel_01");
    assert_eq!(body["status"], "canceled");
}
