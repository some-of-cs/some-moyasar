//! E2E tests for webhook verification gRPC RPCs and `tonic::Status` mapping.

use std::fmt::Write as _;

use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use some_moyasar::MoyasarMockServer;
use some_moyasar::prelude::{ApiErrorType, MoyasarError};
use uuid::Uuid;

use super::proto::VerifyWebhookRpcRequest;
use super::spawn_test_grpc_server;
use super::status_mapper::to_tonic_status;

const TEST_WEBHOOK_SECRET: &str = "whsec_tonic_e2e_secret_1234";

/// Computes the lowercase hex-encoded HMAC-SHA256 signature for a payload.
fn compute_hmac_hex(payload: &[u8], secret: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC-SHA256 accepts any key");
    mac.update(payload);
    let bytes = mac.finalize().into_bytes();
    let mut hex = String::with_capacity(64);
    for b in bytes {
        let _ = write!(&mut hex, "{b:02x}");
    }
    hex
}

/// Builds a sample Moyasar webhook event JSON payload for E2E verification tests.
fn sample_webhook_payload(event_id: Uuid, secret_token: &str) -> Vec<u8> {
    serde_json::json!({
        "id": event_id,
        "type": "payment_paid",
        "created_at": Utc::now().to_rfc3339(),
        "live": false,
        "secret_token": secret_token,
        "data": {
            "id": "pay_tonic_e2e_01",
            "amount": 15000,
            "currency": "SAR"
        }
    })
    .to_string()
    .into_bytes()
}

#[tokio::test]
async fn verify_webhook_valid_hmac_succeeds() {
    let mock = MoyasarMockServer::start().await;
    let (mut client, shutdown_tx) = spawn_test_grpc_server(&mock, TEST_WEBHOOK_SECRET).await;
    let event_id = Uuid::new_v4();
    let raw_body = sample_webhook_payload(event_id, TEST_WEBHOOK_SECRET);
    let signature = compute_hmac_hex(&raw_body, TEST_WEBHOOK_SECRET);

    let response = client
        .verify_webhook(VerifyWebhookRpcRequest {
            signature,
            secret_token: TEST_WEBHOOK_SECRET.to_owned(),
            raw_body,
        })
        .await
        .expect("valid HMAC webhook should verify")
        .into_inner();

    assert_eq!(response.id, event_id.to_string());
    assert_eq!(response.event_type, "payment_paid");
    assert!(!response.live);
    assert!(response.payload_json.contains("pay_tonic_e2e_01"));
    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn verify_webhook_invalid_signature_returns_permission_denied() {
    let mock = MoyasarMockServer::start().await;
    let (mut client, shutdown_tx) = spawn_test_grpc_server(&mock, TEST_WEBHOOK_SECRET).await;
    let raw_body = sample_webhook_payload(Uuid::new_v4(), TEST_WEBHOOK_SECRET);
    let tampered_signature = "0".repeat(64);

    let status = client
        .verify_webhook(VerifyWebhookRpcRequest {
            signature: tampered_signature,
            secret_token: TEST_WEBHOOK_SECRET.to_owned(),
            raw_body,
        })
        .await
        .expect_err("tampered HMAC signature must fail");

    assert_eq!(status.code(), tonic::Code::PermissionDenied);
    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn verify_webhook_missing_signature_returns_permission_denied() {
    let mock = MoyasarMockServer::start().await;
    let (mut client, shutdown_tx) = spawn_test_grpc_server(&mock, TEST_WEBHOOK_SECRET).await;
    let raw_body = sample_webhook_payload(Uuid::new_v4(), TEST_WEBHOOK_SECRET);

    let status = client
        .verify_webhook(VerifyWebhookRpcRequest {
            signature: String::new(),
            secret_token: TEST_WEBHOOK_SECRET.to_owned(),
            raw_body,
        })
        .await
        .expect_err("empty HMAC signature must fail");

    assert_eq!(status.code(), tonic::Code::PermissionDenied);
    let _ = shutdown_tx.send(());
}

#[test]
fn status_mapper_maps_moyasar_errors_without_leakage() {
    let cases = [
        (
            MoyasarError::Api {
                error_type: ApiErrorType::AuthenticationError,
                message: "invalid sk_live_leaked_key provided".to_owned(),
                errors: Vec::new(),
                response_code: None,
            },
            tonic::Code::Unauthenticated,
        ),
        (
            MoyasarError::Api {
                error_type: ApiErrorType::RecordNotFound,
                message: "payment not found".to_owned(),
                errors: Vec::new(),
                response_code: None,
            },
            tonic::Code::NotFound,
        ),
        (
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                message: "amount must be positive".to_owned(),
                errors: Vec::new(),
                response_code: None,
            },
            tonic::Code::InvalidArgument,
        ),
        (MoyasarError::Timeout, tonic::Code::DeadlineExceeded),
        (
            MoyasarError::ServerError {
                status: 503,
                message: "upstream failure sk_test_secret_leak whsec_leak".to_owned(),
            },
            tonic::Code::Unavailable,
        ),
        (
            MoyasarError::InvalidWebhookSignature,
            tonic::Code::PermissionDenied,
        ),
    ];

    for (err, expected_code) in cases {
        let status = to_tonic_status(&err);
        assert_eq!(status.code(), expected_code);
        let msg = status.message();
        assert!(!msg.contains("sk_live_"));
        assert!(!msg.contains("sk_test_"));
        assert!(!msg.contains("whsec_"));
        assert!(!msg.contains("leaked_key"));
        assert!(!msg.contains("secret_leak"));
    }
}
