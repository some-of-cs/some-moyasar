//! E2E tests for Rocket constant-time webhook verification and replay rejection.

use chrono::Utc;
use rocket::http::{ContentType, Status};
use serde_json::{Value, json};
use uuid::Uuid;

/// Default webhook secret token expected by the Rocket test harness.
const TEST_WEBHOOK_SECRET: &str = "whsec_test_secret";

/// Builds a serialized Moyasar webhook event payload for E2E dispatch.
fn build_webhook_payload(event_id: Uuid, secret_token: &str) -> String {
    json!({
        "id": event_id,
        "type": "payment_paid",
        "created_at": Utc::now().to_rfc3339(),
        "secret_token": secret_token,
        "account_name": "Some CS",
        "live": false,
        "data": {
            "id": "pay_e2e_webhook_01",
            "status": "paid",
            "amount": 10000,
            "currency": "SAR"
        }
    })
    .to_string()
}

/// Verifies that `POST /webhooks/moyasar` accepts a payload with a valid `secret_token` (`200 OK`).
#[rocket::async_test]
async fn webhook_valid_secret_accepted() {
    let (client, _mock_server) = super::spawn_test_client().await;
    let event_id = Uuid::new_v4();
    let payload = build_webhook_payload(event_id, TEST_WEBHOOK_SECRET);

    let response = client
        .post("/webhooks/moyasar")
        .header(ContentType::JSON)
        .body(payload)
        .dispatch()
        .await;

    assert_eq!(response.status(), Status::Ok);
    let body_text = response
        .into_string()
        .await
        .expect("webhook 200 response should have a JSON body");
    let body: Value =
        serde_json::from_str(&body_text).expect("webhook 200 body should parse as JSON");
    assert_eq!(body["status"], "accepted");
    assert_eq!(body["event_id"], event_id.to_string());
    assert_eq!(body["event_type"], "payment_paid");
}

/// Verifies that `POST /webhooks/moyasar` rejects a mismatched `secret_token` (`401 Unauthorized`).
#[rocket::async_test]
async fn webhook_invalid_secret_rejected() {
    let (client, _mock_server) = super::spawn_test_client().await;
    let event_id = Uuid::new_v4();
    let payload = build_webhook_payload(event_id, "whsec_invalid_mismatched_secret");

    let response = client
        .post("/webhooks/moyasar")
        .header(ContentType::JSON)
        .body(payload)
        .dispatch()
        .await;

    assert_eq!(response.status(), Status::Unauthorized);
}

/// Verifies that replaying the same `event.id` to `POST /webhooks/moyasar` is rejected (`400`/`409`).
#[rocket::async_test]
async fn webhook_replay_same_event_id_rejected() {
    let (client, _mock_server) = super::spawn_test_client().await;
    let event_id = Uuid::new_v4();
    let payload = build_webhook_payload(event_id, TEST_WEBHOOK_SECRET);

    let first_response = client
        .post("/webhooks/moyasar")
        .header(ContentType::JSON)
        .body(payload.clone())
        .dispatch()
        .await;
    assert_eq!(first_response.status(), Status::Ok);

    let replay_response = client
        .post("/webhooks/moyasar")
        .header(ContentType::JSON)
        .body(payload)
        .dispatch()
        .await;
    assert!(
        matches!(
            replay_response.status(),
            Status::BadRequest | Status::Conflict
        ),
        "expected 400 Bad Request or 409 Conflict for replayed event_id, got {}",
        replay_response.status()
    );
}
