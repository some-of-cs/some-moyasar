//! Payment create, retrieve, and smart 2h void-vs-refund E2E tests.
#![forbid(unsafe_code)]

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use some_moyasar::test_utils::fixtures::FIXTURE_TIMESTAMP;
use tower::ServiceExt;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::spawn_test_app;

/// Builds a valid Moyasar payment JSON payload matching `src/test_utils/mock.rs`.
fn payment_json(id: &str, status: &str, refunded: u64, created_at: &str) -> Value {
    let captured = if status == "voided" { 0_u64 } else { 15000_u64 };
    let captured_fmt = if status == "voided" {
        "0.00 SAR"
    } else {
        "150.00 SAR"
    };
    let refunded_fmt = if refunded > 0 {
        "150.00 SAR"
    } else {
        "0.00 SAR"
    };
    json!({
        "id": id,
        "status": status,
        "amount": 15000,
        "amount_format": "150.00 SAR",
        "fee": 0,
        "fee_format": "0.00 SAR",
        "refunded": refunded,
        "refunded_format": refunded_fmt,
        "captured": captured,
        "captured_format": captured_fmt,
        "currency": "SAR",
        "source": {"type": "creditcard", "company": "mada"},
        "refunded_at": (status == "refunded").then_some(created_at),
        "voided_at": (status == "voided").then_some(created_at),
        "created_at": created_at,
        "updated_at": created_at
    })
}

/// Sends an HTTP request through the Axum router via `tower::ServiceExt::oneshot` and parses JSON.
async fn dispatch_json(
    app: Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
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

/// Verifies `POST /payments` forwards `given_id` and supports deterministic idempotency replay.
#[tokio::test]
async fn e2e_create_payment_with_given_id_idempotency_replay() {
    let (mock, router) = spawn_test_app().await;
    let given_id = "550e8400-e29b-41d4-a716-446655440000";
    let upstream = payment_json("pay_mock_01", "paid", 0, FIXTURE_TIMESTAMP);
    let request_body = json!({
        "amount": 15000,
        "currency": "SAR",
        "source": {"type": "token", "token": "token_mock_01"},
        "given_id": given_id,
        "description": "Axum E2E order #101",
        "callback_url": "https://merchant.example/callback"
    });
    Mock::given(method("POST"))
        .and(path("/v1/payments"))
        .and(body_partial_json(json!({ "given_id": given_id })))
        .respond_with(ResponseTemplate::new(201).set_body_json(upstream))
        .expect(2)
        .mount(mock.server())
        .await;
    let (status_first, first) = dispatch_json(
        router.clone(),
        Method::POST,
        "/payments",
        Some(request_body.clone()),
    )
    .await;
    let (status_replay, replay) =
        dispatch_json(router, Method::POST, "/payments", Some(request_body)).await;
    assert_eq!(status_first, StatusCode::CREATED);
    assert_eq!(status_replay, StatusCode::CREATED);
    assert_eq!(first["id"], "pay_mock_01");
    assert_eq!(first, replay);
}

/// Verifies `GET /payments/{id}` retrieves and returns the Moyasar payment payload.
#[tokio::test]
async fn e2e_retrieve_payment_by_id() {
    let (mock, router) = spawn_test_app().await;
    let upstream = payment_json("pay_mock_01", "paid", 0, FIXTURE_TIMESTAMP);
    Mock::given(method("GET"))
        .and(path("/v1/payments/pay_mock_01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(upstream))
        .expect(1)
        .mount(mock.server())
        .await;
    let (status, body) = dispatch_json(router, Method::GET, "/payments/pay_mock_01", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], "pay_mock_01");
    assert_eq!(body["status"], "paid");
    assert_eq!(body["amount"], 15000);
    assert_eq!(body["currency"], "SAR");
}

/// Verifies `POST /payments/{id}/refund` voids a recent (< 2h) unsettled payment.
#[tokio::test]
async fn e2e_smart_two_hour_void_for_recent_unsettled_payment() {
    let (mock, router) = spawn_test_app().await;
    let recent_ts = (Utc::now() - Duration::minutes(30)).to_rfc3339();
    let fetched = payment_json("pay_recent_01", "paid", 0, &recent_ts);
    let voided = payment_json("pay_recent_01", "voided", 0, &recent_ts);
    Mock::given(method("GET"))
        .and(path("/v1/payments/pay_recent_01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fetched))
        .expect(1)
        .mount(mock.server())
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/payments/pay_recent_01/void"))
        .respond_with(ResponseTemplate::new(200).set_body_json(voided))
        .expect(1)
        .mount(mock.server())
        .await;
    let (status, body) =
        dispatch_json(router, Method::POST, "/payments/pay_recent_01/refund", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], "pay_recent_01");
    assert_eq!(body["status"], "voided");
    assert_eq!(body["refunded"], 0);
}

/// Verifies `POST /payments/{id}/refund` refunds an older (>= 2h) settled payment.
#[tokio::test]
async fn e2e_smart_two_hour_refund_for_older_settled_payment() {
    let (mock, router) = spawn_test_app().await;
    let settled_ts = (Utc::now() - Duration::hours(5)).to_rfc3339();
    let fetched = payment_json("pay_settled_01", "paid", 0, &settled_ts);
    let refunded = payment_json("pay_settled_01", "refunded", 15000, &settled_ts);
    Mock::given(method("GET"))
        .and(path("/v1/payments/pay_settled_01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fetched))
        .expect(1)
        .mount(mock.server())
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/payments/pay_settled_01/refund"))
        .respond_with(ResponseTemplate::new(200).set_body_json(refunded))
        .expect(1)
        .mount(mock.server())
        .await;
    let (status, body) =
        dispatch_json(router, Method::POST, "/payments/pay_settled_01/refund", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], "pay_settled_01");
    assert_eq!(body["status"], "refunded");
    assert_eq!(body["refunded"], 15000);
}
