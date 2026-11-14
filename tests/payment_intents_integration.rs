mod common;

use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use serde_json::{Value, json};
use some_moyasar::prelude::*;

fn sample_intent_json(id: &str, status: &str, created_at: &str, expires_at: &str) -> Value {
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

fn intent_payment_json(id: &str, status: &str, tx_url: Option<&str>) -> Value {
    json!({
        "id": id,
        "status": status,
        "amount": 25000,
        "fee": 0,
        "currency": "SAR",
        "refunded": 0,
        "captured": 0,
        "created_at": "2026-11-14T10:05:00Z",
        "updated_at": "2026-11-14T10:05:00Z",
        "source": {
            "type": "creditcard",
            "transaction_url": tx_url,
            "message": "3DS outcome"
        }
    })
}

#[tokio::test]
async fn payment_intents_create_and_create_with_policy_workflow() {
    let (server, sk_client) = common::start_secret_client().await;
    let intent_id = IntentId::new("pi_550e8400e29b41d4a716446655440000").unwrap();

    common::mount_json_endpoint(
        &server,
        ("POST", "/v1/payment_intents"),
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
        .create(create_params.clone())
        .await
        .unwrap();
    assert_eq!(created.status, PaymentIntentStatus::Created);

    let policy = GivenIdPolicy::Deterministic {
        namespace: MOYASAR_SUBSCRIPTION_NAMESPACE,
        scope: "intent_scope_1".to_string(),
    };
    let created_det = sk_client
        .payment_intents()
        .create_with_policy(create_params, &policy)
        .await
        .unwrap();
    assert_eq!(created_det.id, intent_id);

    let debug_str = format!("{created:?}");
    assert!(!debug_str.contains("pis_secret_token_xyz987"));
    assert!(debug_str.contains("[REDACTED]"));

    let created_time = Utc.with_ymd_and_hms(2026, 11, 14, 10, 0, 0).unwrap();
    assert!(created.is_usable_at(created_time + ChronoDuration::minutes(59)));
    assert!(!created.is_usable_at(created_time + ChronoDuration::minutes(60)));
}

#[tokio::test]
async fn payment_intents_retrieve_200_and_404_not_found_workflow() {
    let (server, sk_client) = common::start_secret_client().await;
    let intent_id = IntentId::new("pi_550e8400e29b41d4a716446655440000").unwrap();
    let get_path = format!("/v1/payment_intents/{}", intent_id.as_str());

    common::mount_json_endpoint(
        &server,
        ("GET", &get_path),
        200,
        sample_intent_json(
            intent_id.as_str(),
            "created",
            "2026-11-14T10:00:00Z",
            "2026-11-14T11:00:00Z",
        ),
    )
    .await;
    let retrieved = sk_client
        .payment_intents()
        .retrieve(&intent_id)
        .await
        .unwrap();
    assert_eq!(retrieved.id, intent_id);

    let missing_id = IntentId::new("pi_00000000000000000000000000000000").unwrap();
    let missing_path = format!("/v1/payment_intents/{}", missing_id.as_str());
    common::mount_json_endpoint(
        &server,
        ("GET", &missing_path),
        404,
        common::api_error_body("record_not_found", "Intent not found", None),
    )
    .await;
    let not_found = sk_client
        .payment_intents()
        .retrieve(&missing_id)
        .await
        .unwrap_err();
    assert!(matches!(
        not_found,
        MoyasarError::Api {
            error_type: ApiErrorType::RecordNotFound,
            ..
        }
    ));
}

#[tokio::test]
async fn payment_intents_fulfill_with_publishable_and_secret_keys_and_outcomes() {
    let (server, pk_client) = common::start_publishable_client().await;
    let sk_client = MoyasarClient::builder(common::resolve_test_secret_key())
        .base_url(server.uri())
        .build()
        .unwrap();
    let intent_id = IntentId::new("pi_550e8400e29b41d4a716446655440000").unwrap();
    let fulfill_path = format!("/v1/payment_intents/{}/fulfill", intent_id.as_str());

    common::mount_json_endpoint(
        &server,
        ("POST", &fulfill_path),
        200,
        intent_payment_json(
            "pay_fulfilled_1",
            "initiated",
            Some("https://api.moyasar.com/v1/3ds/challenge_1"),
        ),
    )
    .await;

    let fulfill_params = FulfillIntentParams::new(
        "pis_secret_token_xyz987",
        FulfillPaymentSourceRequest::Token {
            token: "token_valid_1".to_string(),
        },
    )
    .unwrap();
    let pk_res = pk_client
        .payment_intents()
        .fulfill(&intent_id, fulfill_params.clone())
        .await
        .unwrap();
    let sk_res = sk_client
        .payment_intents()
        .fulfill(&intent_id, fulfill_params)
        .await
        .unwrap();

    assert!(matches!(
        FulfillmentOutcome::from_payment(pk_res),
        FulfillmentOutcome::RequiresAction { .. }
    ));
    assert!(matches!(
        FulfillmentOutcome::from_payment(sk_res),
        FulfillmentOutcome::RequiresAction { .. }
    ));

    for (status, is_paid) in [("paid", true), ("authorized", false), ("failed", false)] {
        let p: IntentPayment =
            serde_json::from_value(intent_payment_json("pay_x", status, None)).unwrap();
        let outcome = FulfillmentOutcome::from_payment(p);
        assert_eq!(matches!(outcome, FulfillmentOutcome::Paid(_)), is_paid);
    }
}

#[tokio::test]
async fn payment_intents_cancel_200_and_400_not_cancelable_workflows() {
    let (server, sk_client) = common::start_secret_client().await;
    let ok_id = IntentId::new("pi_11111111111111111111111111111111").unwrap();
    let ok_cancel_path = format!("/v1/payment_intents/{}/cancel", ok_id.as_str());

    common::mount_json_endpoint(
        &server,
        ("POST", &ok_cancel_path),
        200,
        sample_intent_json(
            ok_id.as_str(),
            "canceled",
            "2026-11-14T10:00:00Z",
            "2026-11-14T11:00:00Z",
        ),
    )
    .await;
    let canceled = sk_client.payment_intents().cancel(&ok_id).await.unwrap();
    assert_eq!(canceled.status, PaymentIntentStatus::Canceled);

    let paid_id = IntentId::new("pi_550e8400e29b41d4a716446655440000").unwrap();
    let err_cancel_path = format!("/v1/payment_intents/{}/cancel", paid_id.as_str());
    common::mount_json_endpoint(
        &server,
        ("POST", &err_cancel_path),
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
        .cancel(&paid_id)
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
