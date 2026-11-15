//! E2E tests for Rocket invoice creation/cancellation and SAMA payouts.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use rocket::http::{ContentType, Status};
use serde_json::{Value, json};
use some_moyasar::prelude::*;
use uuid::Uuid;

use super::{MockResponseSpec, mount_json_endpoint, spawn_test_client};

/// Builds a deterministic Moyasar invoice JSON fixture for mock responses.
fn sample_invoice_json(inv_id: Uuid, status: &str) -> Value {
    json!({
        "id": inv_id,
        "status": status,
        "amount": 50000,
        "currency": "SAR",
        "description": "Rocket E2E Enterprise Plan",
        "logo_url": null,
        "amount_format": "500.00 SAR",
        "url": "https://moyasar.com/i/880e8400",
        "callback_url": "https://merchant.example.com/inv-cb",
        "expired_at": "2026-11-20T10:00:00Z",
        "back_url": null,
        "success_url": null,
        "created_at": "2026-11-15T08:00:00Z",
        "updated_at": "2026-11-15T08:00:00Z",
        "payments": []
    })
}

/// Builds a deterministic Moyasar payout JSON fixture for mock responses.
fn sample_payout_json(payout_id: &str) -> Value {
    json!({
        "id": payout_id,
        "channel": "sarie",
        "status": "paid",
        "amount": 100000,
        "message": "Success",
        "failure_reason": null
    })
}

/// Verifies `POST /invoices` creates an invoice via the tracked Rocket client.
#[tokio::test]
async fn e2e_post_invoices_creates_invoice_via_tracked_client() {
    let (server, client) = spawn_test_client().await;
    let inv_id = Uuid::parse_str("880e8400-e29b-41d4-a716-446655440000").expect("valid uuid");

    mount_json_endpoint(
        &server,
        "POST",
        "/v1/invoices",
        MockResponseSpec {
            status: 201,
            body: sample_invoice_json(inv_id, "created"),
        },
    )
    .await;

    let payload = json!({
        "amount": 50000,
        "currency": "SAR",
        "description": "Rocket E2E Enterprise Plan",
        "callback_url": "https://merchant.example.com/inv-cb"
    });

    let response = client
        .post("/invoices")
        .header(ContentType::JSON)
        .body(payload.to_string())
        .dispatch()
        .await;

    assert_eq!(response.status(), Status::Ok);
    let body: Value = response
        .into_json()
        .await
        .expect("JSON invoice response body");
    assert_eq!(body["id"], inv_id.to_string());
    assert_eq!(body["status"], "created");
    assert_eq!(body["amount"], 50000);
    assert_eq!(body["currency"], "SAR");
}

/// Verifies `POST /invoices/<id>/cancel` cancels the invoice and enforces 29-day SADAD expiry sync.
#[tokio::test]
async fn e2e_post_invoice_cancel_syncs_29_day_sadad_expiry() {
    let (server, client) = spawn_test_client().await;
    let inv_id = Uuid::parse_str("880e8400-e29b-41d4-a716-446655440000").expect("valid uuid");
    let cancel_path = format!("/v1/invoices/{inv_id}/cancel");
    let mut canceled_invoice = sample_invoice_json(inv_id, "canceled");
    canceled_invoice["expired_at"] = json!((Utc::now() + ChronoDuration::days(35)).to_rfc3339());

    mount_json_endpoint(
        &server,
        "PUT",
        &cancel_path,
        MockResponseSpec {
            status: 200,
            body: canceled_invoice,
        },
    )
    .await;

    let before = Utc::now();
    let response = client
        .post(format!("/invoices/{inv_id}/cancel"))
        .dispatch()
        .await;
    let after = Utc::now();

    assert_eq!(response.status(), Status::Ok);
    let body: Value = response.into_json().await.expect("JSON cancel response");
    assert_eq!(body["invoice"]["status"], "canceled");

    let synced_raw = body["sadad_expires_at"]
        .as_str()
        .expect("sadad_expires_at timestamp string");
    let synced: DateTime<Utc> = DateTime::parse_from_rfc3339(synced_raw)
        .expect("valid RFC3339 timestamp")
        .with_timezone(&Utc);
    let min_expected =
        before + ChronoDuration::days(MAX_SADAD_EXPIRY_DAYS) - ChronoDuration::seconds(1);
    let max_expected =
        after + ChronoDuration::days(MAX_SADAD_EXPIRY_DAYS) + ChronoDuration::seconds(1);
    assert!(synced >= min_expected && synced <= max_expected);
}

/// Verifies `POST /payouts` serializes SAMA `PayoutPurpose` cleanly to Moyasar.
#[tokio::test]
async fn e2e_post_payouts_serializes_sama_payout_purpose() {
    let (server, client) = spawn_test_client().await;

    mount_json_endpoint(
        &server,
        "POST",
        "/v1/payouts",
        MockResponseSpec {
            status: 201,
            body: sample_payout_json("po_rocket_001"),
        },
    )
    .await;

    let payload = json!({
        "source_id": "pa_acct_001",
        "sequence_number": "1234567890123456",
        "amount": 100000,
        "purpose": "payment_to_merchant",
        "destination": {
            "type": "bank",
            "iban": "SA4420000001234567891234",
            "name": "Some CS LLC",
            "mobile": "+966551234567",
            "country": "SA",
            "city": "Riyadh"
        },
        "comment": "Vendor settlement"
    });

    let response = client
        .post("/payouts")
        .header(ContentType::JSON)
        .body(payload.to_string())
        .dispatch()
        .await;

    assert_eq!(response.status(), Status::Ok);
    let body: Value = response.into_json().await.expect("JSON payout response");
    assert_eq!(body["id"], "po_rocket_001");
    assert_eq!(body["status"], "paid");

    let recorded = server
        .server()
        .received_requests()
        .await
        .expect("recorded mock requests");
    assert_eq!(recorded.len(), 1);
    let outbound: Value = serde_json::from_slice(&recorded[0].body).expect("valid outbound JSON");
    assert_eq!(
        outbound["purpose"],
        PayoutPurpose::PaymentToMerchant.as_str()
    );
    assert_eq!(outbound["purpose"], "payment_to_merchant");
}

/// Verifies `POST /payouts` with an invalid Saudi IBAN returns `422 Unprocessable Entity`.
#[tokio::test]
async fn e2e_post_payouts_invalid_saudi_iban_returns_422_unprocessable_entity() {
    let (server, client) = spawn_test_client().await;

    let payload = json!({
        "source_id": "pa_acct_001",
        "sequence_number": "1234567890123456",
        "amount": 100000,
        "purpose": "payment_to_merchant",
        "destination": {
            "type": "bank",
            "iban": "US1234567890123456789012",
            "name": "Some CS LLC",
            "mobile": "+966551234567",
            "country": "SA",
            "city": "Riyadh"
        }
    });

    let response = client
        .post("/payouts")
        .header(ContentType::JSON)
        .body(payload.to_string())
        .dispatch()
        .await;

    assert_eq!(response.status(), Status::UnprocessableEntity);
    let body: Value = response.into_json().await.expect("JSON error response");
    assert_eq!(body["error_type"], "invalid_request_error");
    assert!(body["message"].as_str().is_some());

    let recorded = server
        .server()
        .received_requests()
        .await
        .expect("recorded mock requests");
    assert!(recorded.is_empty());
}
