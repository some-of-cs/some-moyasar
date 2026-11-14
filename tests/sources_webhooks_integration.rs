mod common;

use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use some_moyasar::prelude::*;
use uuid::Uuid;

fn sample_webhook_endpoint_json(hook_id: Uuid) -> Value {
    json!({
        "id": hook_id,
        "http_method": "post",
        "url": "https://merchant.example.com/webhooks/moyasar",
        "shared_secret": "whsec_top_secret",
        "events": ["payment_paid", "payment_failed"],
        "created_at": "2026-11-14T10:00:00Z"
    })
}

fn sample_attempt_json(attempt_id: Uuid, hook_id: Uuid) -> Value {
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
    })
}

#[tokio::test]
async fn sources_bin_lookup_and_negative_guards() {
    let (server, sk_client) = common::start_secret_client().await;

    for bad_bin in ["4111111111111111", "41111", "41111a"] {
        assert!(BinLookupParams::new(bad_bin).is_err());
    }
    let bin_params = BinLookupParams::new("411111").unwrap();
    assert!(!format!("{bin_params:?}").contains("411111"));

    common::mount_json_endpoint(
        &server,
        ("POST", "/v1/source/issuer"),
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
}

#[tokio::test]
async fn apple_pay_session_initiation_and_negative_guards() {
    let (server, pk_client) = common::start_publishable_client().await;

    assert!(
        InitiateApplePayParams::new(
            "http://apple-pay-gateway.apple.com/paymentservices/startSession",
            "Some CS",
            "checkout.somecs.com",
            common::TEST_PK,
        )
        .is_err()
    );
    assert!(
        InitiateApplePayParams::new(
            "https://apple-pay-gateway.apple.com/paymentservices/startSession",
            "Some CS",
            "   ",
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
        ("POST", "/v1/applepay/initiate"),
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
async fn webhooks_api_create_list_retrieve_and_available_events_lifecycle() {
    let (server, client) = common::start_secret_client().await;
    let hook_id = Uuid::parse_str("990e8400-e29b-41d4-a716-446655440000").unwrap();
    let hook_json = sample_webhook_endpoint_json(hook_id);

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

    common::mount_json_endpoint(&server, ("POST", "/v1/webhooks"), 201, hook_json.clone()).await;
    common::mount_json_endpoint(
        &server,
        ("GET", "/v1/webhooks"),
        200,
        common::paginated_envelope("webhooks", vec![hook_json.clone()], 1, None),
    )
    .await;
    let hook_path = format!("/v1/webhooks/{hook_id}");
    common::mount_json_endpoint(&server, ("GET", &hook_path), 200, hook_json).await;

    let created = client.webhooks().create(&create_params).await.unwrap();
    assert_eq!(created.id, hook_id);
    assert_eq!(
        client.webhooks().list(Some(1)).await.unwrap().items.len(),
        1
    );
    assert_eq!(
        client.webhooks().retrieve(hook_id).await.unwrap().id,
        hook_id
    );

    common::mount_json_endpoint(
        &server,
        ("GET", "/v1/webhooks/available_events"),
        200,
        json!({ "events": ["payment_paid", "payment_failed"] }),
    )
    .await;
    assert_eq!(client.webhooks().available_events().await.unwrap().len(), 2);

    let (bare_srv, bare_client) = common::start_secret_client().await;
    common::mount_json_endpoint(
        &bare_srv,
        ("GET", "/v1/webhooks/available_events"),
        200,
        json!(["payment_paid", "card_auth_authenticated"]),
    )
    .await;
    assert_eq!(
        bare_client
            .webhooks()
            .available_events()
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn webhooks_api_list_attempts_retrieve_attempt_and_delete_lifecycle() {
    let (server, client) = common::start_secret_client().await;
    let hook_id = Uuid::parse_str("990e8400-e29b-41d4-a716-446655440000").unwrap();
    let attempt_id = Uuid::parse_str("aa0e8400-e29b-41d4-a716-446655440000").unwrap();
    let attempt_json = sample_attempt_json(attempt_id, hook_id);

    common::mount_json_endpoint(
        &server,
        ("GET", "/v1/webhooks/attempts"),
        200,
        common::paginated_envelope("attempts", vec![attempt_json.clone()], 1, None),
    )
    .await;
    let attempt_filter = ListWebhookAttemptsParams::new().page(1).webhook_id(hook_id);
    let attempts_page = client
        .webhooks()
        .list_attempts(Some(&attempt_filter))
        .await
        .unwrap();
    assert_eq!(attempts_page.items.len(), 1);

    let attempt_path = format!("/v1/webhooks/attempts/{attempt_id}");
    common::mount_json_endpoint(&server, ("GET", &attempt_path), 200, attempt_json).await;
    let attempt = client
        .webhooks()
        .retrieve_attempt(attempt_id)
        .await
        .unwrap();
    assert_eq!(attempt.response_code, Some(200));

    let delete_path = format!("/v1/webhooks/{hook_id}");
    common::mount_raw_endpoint(&server, ("DELETE", &delete_path), 204, "").await;
    client.webhooks().delete(hook_id).await.unwrap();
}

#[test]
fn webhook_engine_verifier_and_payment_payload_extractor() {
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
            "fee": 0,
            "currency": "SAR",
            "refunded": 0,
            "captured": 0,
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

    let verified = verifier.verify_and_construct_at(&raw_bytes, now).unwrap();
    assert_eq!(verified.event_type, WebhookEventType::PaymentFailed);
    assert_eq!(
        verified.into_payment().unwrap().status,
        PaymentStatus::Failed
    );
    assert!(verified.into_card_auth().is_err());

    let stale_now = now + ChronoDuration::hours(5);
    assert!(matches!(
        verifier.verify_and_construct_at(&raw_bytes, stale_now),
        Err(WebhookError::TimestampOutOfRange)
    ));
}

#[tokio::test]
async fn webhook_engine_card_auth_extractor_and_deduplicators() {
    let now = Utc.with_ymd_and_hms(2026, 11, 14, 12, 0, 0).unwrap();
    let verifier = ConstantTimeSecretVerifier::new("whsec_engine_secret_123", REPLAY_TOLERANCE);
    let event_id = Uuid::parse_str("bb0e8400-e29b-41d4-a716-446655440000").unwrap();

    let ca_payload = json!({
        "id": event_id,
        "type": "card_auth_authenticated",
        "created_at": "2026-11-14T11:55:00Z",
        "secret_token": "whsec_engine_secret_123",
        "live": false,
        "data": {
            "id": "ca_3ds_999",
            "status": "authenticated",
            "transaction_url": null,
            "result": null
        }
    });
    let ca_event = verifier
        .verify_and_construct_at(&serde_json::to_vec(&ca_payload).unwrap(), now)
        .unwrap();
    assert_eq!(ca_event.into_card_auth().unwrap().id.as_str(), "ca_3ds_999");

    #[derive(Deserialize)]
    struct MinimalId {
        id: String,
    }
    let extracted: MinimalId = ca_event.extract_data().unwrap();
    assert_eq!(extracted.id, "ca_3ds_999");

    let dedup = InMemoryDeduplicator::new();
    assert!(dedup.check_and_record(event_id).await.unwrap());
    assert!(!dedup.check_and_record(event_id).await.unwrap());
    dedup.remove(event_id).await.unwrap();
    assert!(dedup.check_and_record(event_id).await.unwrap());
    dedup.clear();
    assert!(dedup.is_empty());

    let noop = NoopDeduplicator::new();
    assert!(noop.check_and_record(event_id).await.unwrap());
    assert!(noop.check_and_record(event_id).await.unwrap());
}
