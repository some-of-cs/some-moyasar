//! E2E tests for `ConstantTimeSecretVerifier`, `InMemoryDeduplicator`, and `WebhooksService`.

use actix_web::{App, http::StatusCode, test};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde_json::{Value, json};
use some_moyasar::webhook_engine::{REPLAY_TOLERANCE, WebhookEventType};
use uuid::Uuid;

use super::{mount_json_endpoint, paginated_envelope};

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

/// Mounts `WebhooksService` management endpoints on `mock_server`.
async fn mount_webhook_management_mocks(mock_server: &some_moyasar::test_utils::MoyasarMockServer) {
    let endpoint_json = json!({
        "id": "00000000-0000-0000-0000-000000000011",
        "http_method": "post",
        "url": "https://merchant.example/webhooks",
        "events": ["payment_paid"],
        "created_at": "2026-11-01T00:00:00Z"
    });
    mount_json_endpoint(
        mock_server,
        ("POST", "/v1/webhooks"),
        201,
        endpoint_json.clone(),
    )
    .await;
    mount_json_endpoint(
        mock_server,
        ("GET", "/v1/webhooks"),
        200,
        paginated_envelope(json!([endpoint_json])),
    )
    .await;
    mount_json_endpoint(
        mock_server,
        ("GET", "/v1/webhooks/available_events"),
        200,
        json!(["payment_paid", "payment_failed"]),
    )
    .await;
    mount_json_endpoint(
        mock_server,
        ("GET", "/v1/webhooks/attempts"),
        200,
        paginated_envelope(json!([{
            "id": "00000000-0000-0000-0000-000000000022",
            "event_id": "00000000-0000-0000-0000-000000000033",
            "event_type": "payment_paid",
            "response_code": 200,
            "succeeded": true,
            "created_at": "2026-11-01T00:00:00Z"
        }])),
    )
    .await;
}

/// Verifies that `POST /v1/webhooks` with valid secret `"whsec_actix_test_secret"`
/// and legacy `"payment_faild"` typo normalizes to `"payment_failed"` and returns
/// `200 OK` with `"status": "processed"`.
#[actix_rt::test]
async fn webhook_valid_payment_faild_alias_normalizes_and_processes() {
    let (_mock_server, app_state) = super::build_test_harness().await;
    let app = test::init_service(
        App::new()
            .app_data(app_state.clone())
            .configure(super::configure_test_routes),
    )
    .await;

    let payload = build_webhook_payload(
        Uuid::new_v4(),
        "payment_faild",
        TEST_WEBHOOK_SECRET,
        Utc::now(),
    );
    let req = test::TestRequest::post()
        .uri("/v1/webhooks")
        .set_json(&payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["status"], "processed");
    assert_eq!(
        WebhookEventType::from("payment_faild").as_str(),
        "payment_failed"
    );
    assert_eq!(body["event_type"], "payment_failed");
}

/// Verifies that duplicate delivery of the same `event.id` returns `200 OK`
/// with `"status": "duplicate"` via `InMemoryDeduplicator`.
#[actix_rt::test]
async fn webhook_duplicate_event_id_returns_duplicate() {
    let (_mock_server, app_state) = super::build_test_harness().await;
    let app = test::init_service(
        App::new()
            .app_data(app_state.clone())
            .configure(super::configure_test_routes),
    )
    .await;

    let event_id = Uuid::new_v4();
    let payload = build_webhook_payload(event_id, "payment_paid", TEST_WEBHOOK_SECRET, Utc::now());

    let first_req = test::TestRequest::post()
        .uri("/v1/webhooks")
        .set_json(&payload)
        .to_request();
    let first_resp = test::call_service(&app, first_req).await;
    assert_eq!(first_resp.status(), StatusCode::OK);
    let first_body: Value = test::read_body_json(first_resp).await;
    assert_eq!(first_body["status"], "processed");

    let second_req = test::TestRequest::post()
        .uri("/v1/webhooks")
        .set_json(&payload)
        .to_request();
    let second_resp = test::call_service(&app, second_req).await;
    assert_eq!(second_resp.status(), StatusCode::OK);
    let second_body: Value = test::read_body_json(second_resp).await;
    assert_eq!(second_body["status"], "duplicate");
}

/// Verifies that an invalid `secret_token` returns `401 Unauthorized` via
/// `ConstantTimeSecretVerifier`.
#[actix_rt::test]
async fn webhook_wrong_secret_returns_unauthorized() {
    let (_mock_server, app_state) = super::build_test_harness().await;
    let app = test::init_service(
        App::new()
            .app_data(app_state.clone())
            .configure(super::configure_test_routes),
    )
    .await;

    let payload = build_webhook_payload(
        Uuid::new_v4(),
        "payment_paid",
        "whsec_invalid_secret_token",
        Utc::now(),
    );
    let req = test::TestRequest::post()
        .uri("/v1/webhooks")
        .set_json(&payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Verifies that a stale `created_at` timestamp outside the 4-hour
/// `REPLAY_TOLERANCE` window returns `400 Bad Request`.
#[actix_rt::test]
async fn webhook_stale_timestamp_beyond_replay_tolerance_returns_bad_request() {
    let (_mock_server, app_state) = super::build_test_harness().await;
    let app = test::init_service(
        App::new()
            .app_data(app_state.clone())
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
    let req = test::TestRequest::post()
        .uri("/v1/webhooks")
        .set_json(&payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

/// Verifies `WebhooksService::create` and `WebhooksService::list` routes.
#[actix_rt::test]
async fn webhook_endpoints_create_and_list_succeed() {
    let (mock_server, app_state) = super::build_test_harness().await;
    mount_webhook_management_mocks(&mock_server).await;

    let app = test::init_service(
        App::new()
            .app_data(app_state)
            .configure(super::configure_test_routes),
    )
    .await;

    let create_req = test::TestRequest::post()
        .uri("/v1/webhooks/endpoints")
        .set_json(json!({
            "url": "https://merchant.example/webhooks",
            "shared_secret": "whsec_endpoint_secret_01",
            "events": ["payment_paid"]
        }))
        .to_request();
    let create_resp = test::call_service(&app, create_req).await;
    assert_eq!(create_resp.status(), StatusCode::CREATED);

    let list_req = test::TestRequest::get()
        .uri("/v1/webhooks/endpoints")
        .to_request();
    let list_resp = test::call_service(&app, list_req).await;
    assert_eq!(list_resp.status(), StatusCode::OK);
}

/// Verifies `WebhooksService::available_events` and `WebhooksService::list_attempts` routes.
#[actix_rt::test]
async fn webhook_available_events_and_attempts_list_succeed() {
    let (mock_server, app_state) = super::build_test_harness().await;
    mount_webhook_management_mocks(&mock_server).await;

    let app = test::init_service(
        App::new()
            .app_data(app_state)
            .configure(super::configure_test_routes),
    )
    .await;

    let events_req = test::TestRequest::get()
        .uri("/v1/webhooks/available_events")
        .to_request();
    let events_resp = test::call_service(&app, events_req).await;
    assert_eq!(events_resp.status(), StatusCode::OK);

    let attempts_req = test::TestRequest::get()
        .uri("/v1/webhooks/attempts")
        .to_request();
    let attempts_resp = test::call_service(&app, attempts_req).await;
    assert_eq!(attempts_resp.status(), StatusCode::OK);
}
