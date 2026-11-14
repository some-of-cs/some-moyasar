mod common;

use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use serde_json::json;
use some_moyasar::prelude::*;
use uuid::Uuid;

#[tokio::test]
async fn sources_bin_lookup_and_apple_pay_session_initiation_with_negative_guards() {
    let (server, sk_client) = common::start_secret_client().await;
    let pk_client = MoyasarClient::builder(PublishableKey::new(common::TEST_PK).unwrap())
        .base_url(server.uri())
        .build()
        .unwrap();

    assert!(BinLookupParams::new("4111111111111111").is_err());
    assert!(BinLookupParams::new("41111").is_err());
    let bin_params = BinLookupParams::new("411111").unwrap();
    assert!(!format!("{bin_params:?}").contains("411111"));

    common::mount_json_endpoint(
        &server,
        "POST",
        "/v1/source/issuer",
        200,
        json!({
            "issuer_name": "Al Rajhi Bank",
            "issuer_country": "SA",
            "issuer_card_type": "credit",
            "issuer_card_category": "PLATINUM",
            "first_digits": "411111",
            "last_digits": null
        }),
    )
    .await;
    let issuer = sk_client.sources().get_issuer(&bin_params).await.unwrap();
    assert_eq!(issuer.issuer_name.as_deref(), Some("Al Rajhi Bank"));

    assert!(
        InitiateApplePayParams::new(
            "http://apple-pay-gateway.apple.com/paymentservices/startSession",
            "Some CS",
            "checkout.somecs.com",
            common::TEST_PK,
        )
        .is_err()
    );

    let ap_params = InitiateApplePayParams::new(
        "https://apple-pay-gateway.apple.com/paymentservices/startSession",
        "Some CS",
        "checkout.somecs.com",
        common::TEST_PK,
    )
    .unwrap();

    common::mount_json_endpoint(
        &server,
        "POST",
        "/v1/applepay/initiate",
        200,
        json!({
            "epochTimestamp": 1700000000000_u64,
            "expiresAt": 1700003600000_u64,
            "merchantSessionIdentifier": "SSH_SESSION_ID_999",
            "nonce": "nonce_abc",
            "merchantIdentifier": "merchant.com.somecs",
            "domainName": "checkout.somecs.com",
            "displayName": "Some CS",
            "signature": "308006092a864886f7"
        }),
    )
    .await;

    let session = pk_client
        .apple_pay()
        .initiate_session(&ap_params)
        .await
        .unwrap();
    assert_eq!(session.display_name, "Some CS");
    assert_eq!(session.domain_name, "checkout.somecs.com");
}

#[tokio::test]
async fn webhooks_api_crud_available_events_and_attempts_lifecycle() {
    let (server, client) = common::start_secret_client().await;
    let hook_id = Uuid::parse_str("990e8400-e29b-41d4-a716-446655440000").unwrap();
    let attempt_id = Uuid::parse_str("aa0e8400-e29b-41d4-a716-446655440000").unwrap();

    assert!(matches!(
        CreateWebhookParams::new(
            "http://insecure.example.com/webhook",
            WebhookSecret::new("whsec_top_secret"),
        ),
        Err(MoyasarError::InvalidWebhookUrl)
    ));

    let create_params = CreateWebhookParams::new(
        "https://merchant.example.com/webhooks/moyasar",
        WebhookSecret::new("whsec_top_secret"),
    )
    .unwrap()
    .http_method("post")
    .events(vec![
        WebhookEventType::PaymentPaid,
        WebhookEventType::PaymentFailed,
    ]);
    assert!(!format!("{create_params:?}").contains("whsec_top_secret"));

    let hook_json = json!({
        "id": hook_id,
        "http_method": "post",
        "url": "https://merchant.example.com/webhooks/moyasar",
        "shared_secret": "whsec_top_secret",
        "events": ["payment_paid", "payment_failed"],
        "created_at": "2026-11-14T10:00:00Z"
    });
    common::mount_json_endpoint(&server, "POST", "/v1/webhooks", 201, hook_json).await;
    let created = client.webhooks().create(&create_params).await.unwrap();
    assert_eq!(created.id, hook_id);
    assert!(!format!("{created:?}").contains("whsec_top_secret"));

    common::mount_json_endpoint(
        &server,
        "GET",
        "/v1/webhooks/available_events",
        200,
        json!({ "events": ["payment_paid", "payment_failed", "balance_transferred"] }),
    )
    .await;
    let events = client.webhooks().available_events().await.unwrap();
    assert_eq!(events.len(), 3);

    common::mount_json_endpoint(
        &server,
        "GET",
        &format!("/v1/webhooks/attempts/{attempt_id}"),
        200,
        json!({
            "id": attempt_id,
            "webhook_id": hook_id,
            "event_id": Uuid::new_v4(),
            "event_type": "payment_paid",
            "response_code": 200,
            "response_headers": { "content-type": "application/json" },
            "response_body": "{"ok":true}",
            "succeeded": true,
            "message": "OK",
            "created_at": "2026-11-14T10:01:00Z"
        }),
    )
    .await;
    let attempt = client
        .webhooks()
        .retrieve_attempt(attempt_id)
        .await
        .unwrap();
    assert_eq!(attempt.response_code, Some(200));

    common::mount_raw_endpoint(&server, "DELETE", &format!("/v1/webhooks/{hook_id}"), 204, "")
        .await;
    client.webhooks().delete(hook_id).await.unwrap();
}

#[tokio::test]
async fn webhook_engine_constant_time_verifier_replay_guard_deduplicator_and_typo_alias() {
    let now = Utc.with_ymd_and_hms(2026, 11, 14, 12, 0, 0).unwrap();
    let verifier = ConstantTimeSecretVerifier::new("whsec_engine_secret_123", REPLAY_TOLERANCE);
    assert!(!format!("{verifier:?}").contains("whsec_engine_secret_123"));

    let event_id = Uuid::parse_str("bb0e8400-e29b-41d4-a716-446655440000").unwrap();
    let payload = json!({
        "id": event_id,
        "type": "payment_faild",
        "created_at": "2026-11-14T11:55:00Z",
        "secret_token": "whsec_engine_secret_123",
        "account_name": "Some CS",
        "live": false,
        "data": {
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "status": "failed",
            "amount": 10000,
            "amount_format": "100.00 SAR",
            "fee": 0,
            "fee_format": "0.00 SAR",
            "currency": "SAR",
            "refunded": 0,
            "refunded_format": "0.00 SAR",
            "captured": 0,
            "captured_format": "0.00 SAR",
            "created_at": "2026-11-14T11:55:00Z",
            "updated_at": "2026-11-14T11:55:00Z",
            "source": {
                "type": "creditcard",
                "company": "visa",
                "name": "MAHER",
                "number": "4111-11XX-XXXX-1111"
            }
        }
    });
    let raw_bytes = serde_json::to_vec(&payload).unwrap();

    let event = verifier
        .verify_and_construct_at(&raw_bytes, now)
        .unwrap();
    assert_eq!(event.event_type, WebhookEventType::PaymentFailed);
    let payment = event.into_payment().unwrap();
    assert_eq!(payment.status, PaymentStatus::Failed);

    let wrong_verifier = ConstantTimeSecretVerifier::new("wrong_short", REPLAY_TOLERANCE);
    assert!(matches!(
        wrong_verifier.verify_and_construct_at(&raw_bytes, now),
        Err(WebhookError::SecretMismatch)
    ));

    let stale_now = now + ChronoDuration::hours(5);
    assert!(matches!(
        verifier.verify_and_construct_at(&raw_bytes, stale_now),
        Err(WebhookError::TimestampOutOfRange)
    ));
    let future_skew_now = now - ChronoDuration::minutes(15);
    assert!(matches!(
        verifier.verify_and_construct_at(&raw_bytes, future_skew_now),
        Err(WebhookError::TimestampOutOfRange)
    ));

    let dedup = InMemoryDeduplicator::new();
    assert!(dedup.check_and_record(event_id).await.unwrap());
    assert!(!dedup.check_and_record(event_id).await.unwrap());
    dedup.remove(event_id).await.unwrap();
    assert!(dedup.check_and_record(event_id).await.unwrap());
}
