mod common;

use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use serde_json::{Value, json};
use some_moyasar::prelude::*;
use uuid::Uuid;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

fn sample_payment_json(id: &str, status: &str, company: &str, created_at: &str) -> Value {
    json!({
        "id": id,
        "status": status,
        "amount": 10000,
        "amount_format": "100.00 SAR",
        "fee": 200,
        "fee_format": "2.00 SAR",
        "currency": "SAR",
        "refunded": 0,
        "refunded_format": "0.00 SAR",
        "captured": 0,
        "captured_format": "0.00 SAR",
        "description": "Integration Order #42",
        "ip": "203.0.113.10",
        "callback_url": "https://merchant.example.com/callback",
        "created_at": created_at,
        "updated_at": created_at,
        "source": {
            "type": "creditcard",
            "company": company,
            "name": "MAHER MANSOUR",
            "number": "4111-11XX-XXXX-1111",
            "response_code": "00"
        }
    })
}

#[tokio::test]
async fn payments_create_retrieve_fetch_and_capture_lifecycle() {
    let (server, client) = common::start_secret_client().await;
    let pay_uuid = "550e8400-e29b-41d4-a716-446655440000";

    common::mount_json_endpoint(
        &server,
        ("POST", "/v1/payments"),
        201,
        sample_payment_json(pay_uuid, "authorized", "visa", "2026-11-14T10:00:00Z"),
    )
    .await;

    let token_src = PaymentSourceRequest::Token(TokenSource::new("token_abc123").unwrap());
    let create_params = CreatePaymentParams::builder()
        .amount(10000)
        .currency(Currency::SAR)
        .payment_source(token_src)
        .description("Integration Order #42")
        .callback_url("https://merchant.example.com/callback")
        .build()
        .unwrap();

    let created = client.payments().create(&create_params).await.unwrap();
    assert_eq!(created.status, PaymentStatus::Authorized);

    let pay_id = PaymentId::new(format!("pay_{pay_uuid}")).unwrap();
    let get_path = format!("/v1/payments/pay_{pay_uuid}");
    common::mount_json_endpoint(
        &server,
        ("GET", &get_path),
        200,
        sample_payment_json(pay_uuid, "authorized", "visa", "2026-11-14T10:00:00Z"),
    )
    .await;
    assert_eq!(
        client.payments().retrieve(&pay_id).await.unwrap().id,
        pay_uuid
    );
    assert_eq!(client.payments().fetch(&pay_id).await.unwrap().id, pay_uuid);

    let capture_path = format!("/v1/payments/{pay_uuid}/capture");
    common::mount_json_endpoint(
        &server,
        ("POST", &capture_path),
        200,
        sample_payment_json(pay_uuid, "captured", "visa", "2026-11-14T10:05:00Z"),
    )
    .await;
    let partial = client
        .payments()
        .capture(pay_uuid, Some(5000))
        .await
        .unwrap();
    let full = client.payments().capture(pay_uuid, None).await.unwrap();
    assert_eq!(partial.status, PaymentStatus::Captured);
    assert_eq!(full.status, PaymentStatus::Captured);
}

#[tokio::test]
async fn payments_update_void_and_refund_lifecycle() {
    let (server, client) = common::start_secret_client().await;
    let pay_uuid = "550e8400-e29b-41d4-a716-446655440000";
    let update_path = format!("/v1/payments/{pay_uuid}");

    common::mount_json_endpoint(
        &server,
        ("PUT", &update_path),
        200,
        sample_payment_json(pay_uuid, "paid", "visa", "2026-11-14T10:05:00Z"),
    )
    .await;
    let updated = client
        .payments()
        .update(
            pay_uuid,
            &UpdatePaymentParams::new().description("Updated Description"),
        )
        .await
        .unwrap();
    assert_eq!(updated.status, PaymentStatus::Paid);

    let void_path = format!("/v1/payments/{pay_uuid}/void");
    common::mount_json_endpoint(
        &server,
        ("POST", &void_path),
        200,
        sample_payment_json(pay_uuid, "voided", "visa", "2026-11-14T10:06:00Z"),
    )
    .await;
    assert_eq!(
        client.payments().void(pay_uuid).await.unwrap().status,
        PaymentStatus::Voided
    );

    let refund_path = format!("/v1/payments/{pay_uuid}/refund");
    common::mount_json_endpoint(
        &server,
        ("POST", &refund_path),
        200,
        sample_payment_json(pay_uuid, "refunded", "visa", "2026-11-14T10:07:00Z"),
    )
    .await;
    let direct_ref = client.payments().refund(pay_uuid, Some(4000)).await.unwrap();
    let checked_ref = client
        .payments()
        .refund_payment(&updated, Some(4000))
        .await
        .unwrap();
    assert_eq!(direct_ref.status, PaymentStatus::Refunded);
    assert_eq!(checked_ref.status, PaymentStatus::Refunded);
}

#[tokio::test]
async fn payments_over_refund_guard_and_filtered_list() {
    let (server, client) = common::start_secret_client().await;
    let pay_uuid = "550e8400-e29b-41d4-a716-446655440000";
    let paid: Payment = serde_json::from_value(sample_payment_json(
        pay_uuid,
        "paid",
        "visa",
        "2026-11-14T10:05:00Z",
    ))
    .unwrap();

    let over_refund_err = client
        .payments()
        .refund_payment(&paid, Some(25_000))
        .await
        .unwrap_err();
    assert!(matches!(over_refund_err, MoyasarError::Api { .. }));

    Mock::given(method("GET"))
        .and(path("/v1/payments"))
        .and(query_param("page", "1"))
        .and(query_param("status", "paid"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(common::paginated_envelope(
                "payments",
                vec![sample_payment_json(
                    pay_uuid,
                    "paid",
                    "visa",
                    "2026-11-14T10:05:00Z",
                )],
                1,
                None,
            )),
        )
        .mount(&server)
        .await;

    let list_params = ListParams::new().page(1).status(PaymentStatus::Paid);
    let list = client.payments().list(&list_params).await.unwrap();
    assert_eq!(list.items.len(), 1);
}

#[tokio::test]
async fn smart_reversal_plans_mada_14d_hold_and_falls_back_from_void_400_to_refund() {
    let (server, client) = common::start_secret_client().await;
    let pay_uuid = "660e8400-e29b-41d4-a716-446655440000";
    let created_at = Utc.with_ymd_and_hms(2026, 11, 14, 10, 0, 0).unwrap();
    let within_2h = created_at + ChronoDuration::minutes(45);
    let after_15d = created_at + ChronoDuration::days(15);

    let mada_authorized: Payment = serde_json::from_value(sample_payment_json(
        pay_uuid,
        "authorized",
        "mada",
        "2026-11-14T10:00:00Z",
    ))
    .unwrap();

    let plan = client
        .payments()
        .build_reversal_plan(&mada_authorized, within_2h);
    assert_eq!(plan.strategy, ReversalStrategy::Void);
    assert!(!plan.mada_hold_expired);
    assert!(
        client
            .payments()
            .is_mada_hold_expired(&mada_authorized, after_15d)
    );

    let mada_paid: Payment = serde_json::from_value(sample_payment_json(
        pay_uuid,
        "paid",
        "mada",
        "2026-11-14T10:00:00Z",
    ))
    .unwrap();
    let void_path = format!("/v1/payments/{pay_uuid}/void");
    let refund_path = format!("/v1/payments/{pay_uuid}/refund");
    common::mount_json_endpoint(
        &server,
        ("POST", &void_path),
        400,
        common::api_error_body("invalid_request_error", "Void window elapsed", None),
    )
    .await;
    common::mount_json_endpoint(
        &server,
        ("POST", &refund_path),
        200,
        sample_payment_json(pay_uuid, "refunded", "mada", "2026-11-14T10:00:00Z"),
    )
    .await;

    let reversed = client
        .payments()
        .reverse(&mada_paid, within_2h)
        .await
        .unwrap();
    assert_eq!(reversed.status, PaymentStatus::Refunded);
}

#[tokio::test]
async fn stc_pay_otp_verification_succeeds_on_200_and_rejects_non_https_or_invalid_otp() {
    let (server, client) = common::start_secret_client().await;
    let otp_path = "/v1/stc_pays/pay_123/proceed";

    common::mount_json_endpoint(
        &server,
        ("POST", otp_path),
        200,
        json!({
            "id": "pay_123",
            "status": "paid",
            "amount": 10000,
            "currency": "SAR"
        }),
    )
    .await;

    let valid_url = format!("{}{otp_path}?otp_token=tok_xyz", server.uri());
    let verified = client
        .payments()
        .verify_stc_otp(&valid_url, "123456")
        .await
        .unwrap();
    assert_eq!(verified.id, "pay_123");
    assert_eq!(verified.status, "paid");

    let http_err = client
        .payments()
        .verify_stc_otp("http://api.moyasar.com/v1/stc/verify", "123456")
        .await
        .unwrap_err();
    assert!(matches!(http_err, MoyasarError::InvalidBaseUrl));

    let bad_otp_err = client
        .payments()
        .verify_stc_otp("https://api.moyasar.com/v1/stc/verify", "   ")
        .await
        .unwrap_err();
    assert!(matches!(bad_otp_err, MoyasarError::Api { .. }));
}

#[test]
fn polymorphic_sources_enforce_card_wallets_stcpay_and_sadad_validations() {
    let now = Utc.with_ymd_and_hms(2026, 11, 14, 12, 0, 0).unwrap();

    assert!(matches!(
        CreditCardSource::new("Maher", "4111111111111112", 12, 2029, "123"),
        Err(MoyasarError::InvalidCardNumber)
    ));
    assert!(matches!(
        validate_expiry_against(10, 2026, 2026, 11),
        Err(MoyasarError::CardExpired)
    ));
    assert!(CreditCardSource::new("Maher", "378282246310005", 12, 2029, "1234").is_ok());

    assert!(ApplePaySource::new("apple_tok_123").validate().is_ok());
    assert!(SamsungPaySource::new("samsung_tok_123").validate().is_ok());
    assert!(ApplePaySource::new("   ").validate().is_err());

    assert_eq!(
        normalize_saudi_mobile("+966551234567").unwrap(),
        "0551234567"
    );
    assert!(StcPaySource::new("0551234567").is_ok());
    assert!(matches!(
        StcPaySource::new("0112345678"),
        Err(MoyasarError::InvalidMobile)
    ));

    let sadad = SadadBillSource::new("NationalId123").with_biller_code("001");
    assert!(sadad.validate(now).is_ok());
    let too_far = now + ChronoDuration::days(30);
    assert!(matches!(
        validate_sadad_expiry(Some(too_far), now),
        Err(MoyasarError::SadadExpiryTooFar)
    ));
}

#[test]
fn marketplace_splits_and_aft_sender_enforce_validation_and_redaction() {
    let ent_1 = Uuid::new_v4();
    let ent_2 = Uuid::new_v4();
    let s1 =
        PaymentSplitRequest::new(6000, SplitRecipientType::Entity, ent_1).with_fee_source(true);
    let s2 =
        PaymentSplitRequest::new(6000, SplitRecipientType::Entity, ent_2).with_fee_source(true);
    assert!(matches!(
        validate_splits(&[s1.clone(), s2], 15000),
        Err(MoyasarError::MultipleFeeSources)
    ));
    let s3 = PaymentSplitRequest::new(6000, SplitRecipientType::Entity, ent_2);
    assert!(matches!(
        validate_splits(&[s1, s3], 10000),
        Err(MoyasarError::SplitTotalExceedsAmount)
    ));

    let resp_splits = [
        PaymentSplitResponse {
            id: None,
            amount: 6000,
            recipient_type: SplitRecipientType::Entity,
            recipient_id: ent_1,
            reference: None,
            description: None,
            fee_source: Some(true),
            refundable: true,
            status: None,
        },
        PaymentSplitResponse {
            id: None,
            amount: 6000,
            recipient_type: SplitRecipientType::Entity,
            recipient_id: ent_2,
            reference: None,
            description: None,
            fee_source: None,
            refundable: true,
            status: None,
        },
    ];
    let refunds = calculate_proportional_refunds(&resp_splits, 12000, 3000);
    assert_eq!(refunds.iter().map(|s| s.refund_amount).sum::<u64>(), 3000);

    let sender = AftSender::new("Maher", "Mansour")
        .with_nationality("SAU")
        .with_id("national_id", "1098765432");
    let debug_str = format!("{sender:?}");
    assert!(!debug_str.contains("1098765432"));
    assert!(debug_str.contains("[REDACTED]"));
}

#[tokio::test]
async fn payments_create_retries_503_with_deterministic_given_id_and_rejects_malformed_201() {
    let (server, client) = common::start_secret_client_with_retries(1).await;
    let pay_uuid = "770e8400-e29b-41d4-a716-446655440000";

    Mock::given(method("POST"))
        .and(path("/v1/payments"))
        .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    common::mount_json_endpoint(
        &server,
        ("POST", "/v1/payments"),
        201,
        sample_payment_json(pay_uuid, "paid", "visa", "2026-11-14T10:00:00Z"),
    )
    .await;

    let token_src = PaymentSourceRequest::Token(TokenSource::new("token_retry123").unwrap());
    let params = CreatePaymentParams::builder()
        .amount(10000)
        .currency(Currency::SAR)
        .payment_source(token_src)
        .build()
        .unwrap();
    let expected_given_id = derive_subscription_given_id("sub_99", 1);
    let policy = GivenIdPolicy::Deterministic {
        namespace: MOYASAR_SUBSCRIPTION_NAMESPACE,
        scope: "sub_99:1".to_string(),
    };
    let payment = client
        .payments()
        .create_with_policy(&params, &policy)
        .await
        .unwrap();
    assert_eq!(payment.status, PaymentStatus::Paid);

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 2);
    for req in &received {
        let body: Value = serde_json::from_slice(&req.body).unwrap();
        assert_eq!(body["given_id"], expected_given_id.to_string());
    }

    let (bad_server, bad_client) = common::start_secret_client_with_retries(2).await;
    common::mount_raw_endpoint(&bad_server, ("POST", "/v1/payments"), 201, "{invalid_json").await;
    let err = bad_client.payments().create(&params).await.unwrap_err();
    assert!(matches!(
        err,
        MoyasarError::Api {
            error_type: ApiErrorType::Unknown,
            ..
        }
    ));
    assert!(!err.is_retryable());
    assert_eq!(bad_server.received_requests().await.unwrap().len(), 1);
}
