mod common;

use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use serde_json::json;
use some_moyasar::prelude::*;

fn sample_intent_json(
    id: &str,
    status: &str,
    created_at: &str,
    expires_at: &str,
) -> serde_json::Value {
    json!({
        "id": id,
        "status": status,
        "amount": 25000,
        "currency": "SAR",
        "description": "Checkout Session #10",
        "client_secret": "pis_secret_token_xyz987",
        "callback_url": "https://merchant.example.com/pi-callback",
        "expires_at": expires_at,
        "created_at": created_at,
        "updated_at": created_at
    })
}

#[tokio::test]
async fn payment_intents_create_retrieve_fulfill_and_cancel_workflows() {
    let (server, sk_client) = common::start_secret_client().await;
    let intent_id = IntentId::new("pi_550e8400e29b41d4a716446655440000").unwrap();

    common::mount_json_endpoint(
        &server,
        "POST",
        "/v1/payment_intents",
        201,
        sample_intent_json(
            intent_id.as_str(),
            "created",
            "2026-11-14T10:00:00Z",
            "2026-11-14T11:00:00Z",
        ),
    )
    .await;

    let create_params = CreatePaymentIntentParams::new(
        25000,
        Currency::SAR,
        "https://merchant.example.com/pi-callback",
    );
    let created = sk_client
        .payment_intents()
        .create(create_params)
        .await
        .unwrap();
    assert_eq!(created.status, PaymentIntentStatus::Created);

    let debug_str = format!("{created:?}");
    assert!(!debug_str.contains("pis_secret_token_xyz987"));
    assert!(debug_str.contains("[REDACTED]"));

    let created_time = Utc.with_ymd_and_hms(2026, 11, 14, 10, 0, 0).unwrap();
    assert!(created.is_usable_at(created_time + ChronoDuration::minutes(59)));
    assert!(!created.is_usable_at(created_time + ChronoDuration::minutes(60)));

    let pk_client = MoyasarClient::builder(PublishableKey::new(common::TEST_PK).unwrap())
        .base_url(server.uri())
        .build()
        .unwrap();

    common::mount_json_endpoint(
        &server,
        "POST",
        &format!("/v1/payment_intents/{}/fulfill", intent_id.as_str()),
        200,
        json!({
            "id": "pay_fulfilled_1",
            "status": "initiated",
            "amount": 25000,
            "fee": 0,
            "currency": "SAR",
            "refunded": 0,
            "captured": 0,
            "created_at": "2026-11-14T10:05:00Z",
            "updated_at": "2026-11-14T10:05:00Z",
            "source": {
                "type": "creditcard",
                "transaction_url": "https://api.moyasar.com/v1/3ds/challenge_1"
            }
        }),
    )
    .await;

    let fulfill_params = FulfillIntentParams::new(
        "pis_secret_token_xyz987",
        FulfillPaymentSourceRequest::Token {
            token: "token_valid_1".to_string(),
        },
    )
    .unwrap();
    let fulfilled = pk_client
        .payment_intents()
        .fulfill(&intent_id, fulfill_params)
        .await
        .unwrap();
    let outcome = FulfillmentOutcome::from_payment(fulfilled);
    assert!(matches!(
        outcome,
        FulfillmentOutcome::RequiresAction {
            transaction_url: Some(ref url),
            ..
        } if url == "https://api.moyasar.com/v1/3ds/challenge_1"
    ));

    common::mount_json_endpoint(
        &server,
        "POST",
        &format!("/v1/payment_intents/{}/cancel", intent_id.as_str()),
        400,
        common::api_error_body(
            "intent_not_cancelable",
            "Payment intent cannot be canceled in paid status",
            None,
        ),
    )
    .await;

    let cancel_err = sk_client
        .payment_intents()
        .cancel(&intent_id)
        .await
        .unwrap_err();
    assert!(matches!(
        cancel_err,
        MoyasarError::Api {
            error_type: ApiErrorType::IntentNotCancelable,
            ..
        }
    ));
}

#[test]
fn payment_intents_negative_guards_reject_invalid_ids_blank_secrets_and_expired_intents() {
    assert!(IntentId::new("   ").is_err());
    assert!(IntentId::new("pi_").is_err());
    assert!(IntentId::new("pi_../escape").is_err());

    assert!(validate_client_secret("   ").is_err());
    assert!(validate_client_secret("pis_valid_secret").is_ok());

    let expired_intent: PaymentIntent = serde_json::from_value(sample_intent_json(
        "pi_12345",
        "created",
        "2020-01-01T10:00:00Z",
        "2020-01-01T11:00:00Z",
    ))
    .unwrap();
    assert!(validate_pre_fulfillment(&expired_intent, "pis_secret_token_xyz987").is_err());
    assert!(validate_pre_fulfillment(&expired_intent, "   ").is_err());
}
