//! End-to-end tests for the Moyasar Axum webhook endpoint (US-AXUM-2.0 Unit 3).
//!
//! Exercises constant-time Double-HMAC `secret_token` verification,
//! replay-window timestamp enforcement against [`REPLAY_TOLERANCE`], and
//! duplicate `event.id` suppression via [`tower::ServiceExt::oneshot`].

#![forbid(unsafe_code)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::response::Response;
use chrono::{TimeDelta, Utc};
use serde_json::Value;
use some_moyasar::test_utils::fixtures::webhook_event_fixture_at;
use some_moyasar::webhook_engine::timestamp::REPLAY_TOLERANCE;
use tower::ServiceExt;
use uuid::Uuid;

const WEBHOOK_PATH: &str = "/webhooks/moyasar";

/// Builds a JSON `POST /webhooks/moyasar` request from a [`Value`] payload.
fn build_webhook_request(payload: &Value) -> Request<Body> {
    let body_bytes = serde_json::to_vec(payload).expect("webhook fixture must serialize");
    Request::builder()
        .method("POST")
        .uri(WEBHOOK_PATH)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body_bytes))
        .expect("valid webhook HTTP request")
}

/// Dispatches a single webhook payload through `app` via [`ServiceExt::oneshot`].
async fn send_webhook(app: Router, payload: &Value) -> Response {
    app.oneshot(build_webhook_request(payload))
        .await
        .expect("oneshot webhook request should not fail")
}

/// Verifies that a webhook with a matching `secret_token` and fresh `created_at`
/// timestamp passes constant-time Double-HMAC verification and returns `200 OK`.
#[tokio::test]
async fn valid_double_hmac_webhook_returns_ok() {
    let app = super::spawn_test_app().await;
    let fresh_ts = Utc::now().to_rfc3339();
    let payload = webhook_event_fixture_at("payment_paid", super::TEST_WEBHOOK_SECRET, &fresh_ts);

    let response = send_webhook(app, &payload).await;
    assert_eq!(response.status(), StatusCode::OK);
}

/// Verifies that a webhook with an incorrect `secret_token` is rejected with
/// `401 Unauthorized` by the constant-time Double-HMAC verifier.
#[tokio::test]
async fn wrong_secret_webhook_returns_unauthorized() {
    let app = super::spawn_test_app().await;
    let fresh_ts = Utc::now().to_rfc3339();
    let payload = webhook_event_fixture_at("payment_paid", "whsec_wrong_secret_value", &fresh_ts);

    let response = send_webhook(app, &payload).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Verifies that a webhook whose `created_at` timestamp is older than
/// [`REPLAY_TOLERANCE`] is rejected with `401 Unauthorized`.
#[tokio::test]
async fn stale_timestamp_webhook_returns_unauthorized() {
    let app = super::spawn_test_app().await;
    let tolerance_secs = i64::try_from(REPLAY_TOLERANCE.as_secs()).unwrap_or(14_400);
    let stale_ts = (Utc::now() - TimeDelta::seconds(tolerance_secs + 60)).to_rfc3339();
    let payload = webhook_event_fixture_at("payment_paid", super::TEST_WEBHOOK_SECRET, &stale_ts);

    let response = send_webhook(app, &payload).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Verifies that a duplicate webhook delivery with the same `event.id` is
/// processed on the first request and suppressed cleanly on the second request.
#[tokio::test]
async fn duplicate_event_id_is_suppressed_cleanly() {
    let app = super::spawn_test_app().await;
    let fresh_ts = Utc::now().to_rfc3339();
    let mut payload =
        webhook_event_fixture_at("payment_paid", super::TEST_WEBHOOK_SECRET, &fresh_ts);
    payload["id"] = Value::String(Uuid::new_v4().to_string());

    let first_response = send_webhook(app.clone(), &payload).await;
    assert_eq!(first_response.status(), StatusCode::OK);

    let second_response = send_webhook(app, &payload).await;
    assert!(second_response.status().is_success());
}
