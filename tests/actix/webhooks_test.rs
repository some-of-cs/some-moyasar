//! E2E tests for `ConstantTimeSecretVerifier`, `"payment_faild"` alias normalization, and replay window.

use actix_web::{App, http::StatusCode, test};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde_json::{Value, json};
use some_moyasar::webhook_engine::{REPLAY_TOLERANCE, WebhookEventType};
use uuid::Uuid;

/// Shared webhook secret configured in the Actix-Web test harness.
const TEST_WEBHOOK_SECRET: &str = "whsec_actix_test_secret";

/// Builds a Moyasar webhook JSON payload for E2E Actix-Web tests.
fn build_webhook_payload(
    event_id: Uuid,
    event_type: &str,
    secret_token: &str,
    created_at: DateTime<Utc>,
) -> Value {
    json!({
        "id": event_id,
        "type": event_type,
        "created_at": created_at.to_rfc3339(),
        "secret_token": secret_token,
        "account_name": "some_cs_actix_merchant",
        "live": false,
        "data": {
            "id": "pay_actix_webhook_01",
            "status": "failed",
            "amount": 15000,
            "currency": "SAR"
        }
    })
}

/// Sends a `POST /v1/webhooks` JSON request to the initialized Actix-Web service.
async fn post_webhook_json<S, B, E>(app: &S, payload: &Value) -> actix_web::dev::ServiceResponse<B>
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = E,
        >,
    E: std::fmt::Debug,
{
    let req = test::TestRequest::post()
        .uri("/v1/webhooks")
        .set_json(payload)
        .to_request();
    test::call_service(app, req).await
}

/// Verifies that `POST /v1/webhooks` with valid secret `"whsec_actix_test_secret"`
/// and legacy `"payment_faild"` typo normalizes to `"payment_failed"` and returns
/// `200 OK` with `"status": "processed"`.
#[actix_rt::test]
async fn webhook_valid_payment_faild_alias_normalizes_and_processes() {
    let harness = super::build_test_harness().await;
    let app = test::init_service(
        App::new()
            .app_data(harness.state.clone())
            .configure(super::configure_test_routes),
    )
    .await;

    let payload = build_webhook_payload(
        Uuid::new_v4(),
        "payment_faild",
        TEST_WEBHOOK_SECRET,
        Utc::now(),
    );
    let resp = post_webhook_json(&app, &payload).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["status"], "processed");
    assert_eq!(
        WebhookEventType::from("payment_faild").as_str(),
        "payment_failed"
    );
    if let Some(normalized) = body.get("event_type").or_else(|| body.get("type")) {
        assert_eq!(normalized, "payment_failed");
    }
}

/// Verifies that duplicate delivery of the same `event.id` returns `200 OK`
/// with `"status": "duplicate"` via `InMemoryDeduplicator`.
#[actix_rt::test]
async fn webhook_duplicate_event_id_returns_duplicate() {
    let harness = super::build_test_harness().await;
    let app = test::init_service(
        App::new()
            .app_data(harness.state.clone())
            .configure(super::configure_test_routes),
    )
    .await;

    let event_id = Uuid::new_v4();
    let payload = build_webhook_payload(event_id, "payment_paid", TEST_WEBHOOK_SECRET, Utc::now());

    let first_resp = post_webhook_json(&app, &payload).await;
    assert_eq!(first_resp.status(), StatusCode::OK);
    let first_body: Value = test::read_body_json(first_resp).await;
    assert_eq!(first_body["status"], "processed");

    let second_resp = post_webhook_json(&app, &payload).await;
    assert_eq!(second_resp.status(), StatusCode::OK);
    let second_body: Value = test::read_body_json(second_resp).await;
    assert_eq!(second_body["status"], "duplicate");
}

/// Verifies that an invalid `secret_token` returns `401 Unauthorized` via
/// `ConstantTimeSecretVerifier`.
#[actix_rt::test]
async fn webhook_wrong_secret_returns_unauthorized() {
    let harness = super::build_test_harness().await;
    let app = test::init_service(
        App::new()
            .app_data(harness.state.clone())
            .configure(super::configure_test_routes),
    )
    .await;

    let payload = build_webhook_payload(
        Uuid::new_v4(),
        "payment_paid",
        "whsec_invalid_secret_token",
        Utc::now(),
    );
    let resp = post_webhook_json(&app, &payload).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Verifies that a stale `created_at` timestamp outside the 4-hour
/// `REPLAY_TOLERANCE` window returns `400 Bad Request`.
#[actix_rt::test]
async fn webhook_stale_timestamp_beyond_replay_tolerance_returns_bad_request() {
    let harness = super::build_test_harness().await;
    let app = test::init_service(
        App::new()
            .app_data(harness.state.clone())
            .configure(super::configure_test_routes),
    )
    .await;

    let tolerance = ChronoDuration::from_std(REPLAY_TOLERANCE).expect("valid replay tolerance");
    let stale_timestamp = Utc::now() - tolerance - ChronoDuration::seconds(60);
    let payload = build_webhook_payload(
        Uuid::new_v4(),
        "payment_paid",
        TEST_WEBHOOK_SECRET,
        stale_timestamp,
    );
    let resp = post_webhook_json(&app, &payload).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
