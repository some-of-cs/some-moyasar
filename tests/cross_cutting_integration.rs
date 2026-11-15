mod common;

use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use serde_json::{Value, json};
use some_moyasar::prelude::*;
use uuid::Uuid;
use wiremock::matchers::{body_json, body_partial_json, body_string, method, path};
use wiremock::{Mock, ResponseTemplate};

fn sample_module8_payment_json(id: &str, status: &str, refunded: u64, created_at: &str) -> Value {
    let refunded_fmt = match refunded {
        5000 => "50.00 SAR",
        19934 => "199.34 SAR",
        29900 => "299.00 SAR",
        _ => "0.00 SAR",
    };
    let captured = if status == "voided" { 0 } else { 29900 };
    let captured_fmt = if status == "voided" {
        "0.00 SAR"
    } else {
        "299.00 SAR"
    };
    json!({
        "id": id,
        "status": status,
        "amount": 29900,
        "amount_format": "299.00 SAR",
        "fee": 0,
        "fee_format": "0.00 SAR",
        "refunded": refunded,
        "refunded_format": refunded_fmt,
        "captured": captured,
        "captured_format": captured_fmt,
        "currency": "SAR",
        "source": { "type": "token" },
        "splits": [
            { "id": "sp_1", "amount": 10000, "refunded": true },
            { "id": "sp_2", "amount": 19900, "refunded": false }
        ],
        "refunded_at": (status == "refunded").then_some(created_at),
        "voided_at": (status == "voided").then_some(created_at),
        "created_at": created_at,
        "updated_at": created_at
    })
}

fn sample_token_json(id: &str, status: &str, created_at: &str) -> Value {
    json!({
        "id": id,
        "status": status,
        "number": "411111XXXXXX1111",
        "company": "visa",
        "gateway_id": "gw_1",
        "created_at": created_at
    })
}

fn assert_unified_capabilities<T>(_gateway: &T)
where
    T: PaymentCapability + WebhookCapability + WebhookVerifier + PayoutEngine,
{
}

async fn mount_refund_and_void_mocks(server: &wiremock::MockServer, pay_id: &str, recent: &str) {
    let get_path = format!("/v1/payments/{pay_id}");
    let refund_path = format!("/v1/payments/{pay_id}/refund");
    let void_path = format!("/v1/payments/{pay_id}/void");
    common::mount_json_endpoint(
        server,
        ("GET", &get_path),
        200,
        sample_module8_payment_json(pay_id, "paid", 0, recent),
    )
    .await;
    Mock::given(method("POST"))
        .and(path(&refund_path))
        .and(body_json(json!({ "amount": 5000 })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(sample_module8_payment_json(
                pay_id, "refunded", 5000, recent,
            )),
        )
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path(&refund_path))
        .and(body_string(""))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(sample_module8_payment_json(
                pay_id, "refunded", 29900, recent,
            )),
        )
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path(&void_path))
        .respond_with(
            ResponseTemplate::new(422).set_body_json(common::api_error_body(
                "invalid_request_error",
                "Already settled",
                None,
            )),
        )
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn stripe_compat_refunds_service_partial_void_fallback_and_over_refund_guard() {
    let (server, client) = common::start_secret_client().await;
    let recent = (Utc::now() - ChronoDuration::minutes(20)).to_rfc3339();
    let pay_id = "pay_m8_ref_01";
    mount_refund_and_void_mocks(&server, pay_id, &recent).await;

    let partial = client
        .refunds()
        .create(RefundCreateParams::new(pay_id).amount(5000))
        .await
        .unwrap();
    assert_eq!(partial.payment_id, pay_id);
    assert_eq!(partial.refunded_amount, 5000);
    assert_eq!(partial.split_reversal_flags, vec![true, false]);

    let fallback = client
        .refunds()
        .create(RefundCreateParams::new(pay_id).prefer_void_if_unsettled(true))
        .await
        .unwrap();
    assert_eq!(fallback.status, PaymentStatus::Refunded);
    assert_eq!(fallback.refunded_amount, 29900);

    assert!(
        client
            .refunds()
            .create(RefundCreateParams::new(pay_id).amount(99_999))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn stripe_compat_customers_service_and_in_memory_vault_lifecycle() {
    let (server, client) = common::start_secret_client().await;
    let vault = InMemoryCustomerVault::with_capacity_limits(2, 2);
    assert_eq!(vault.max_customers(), 2);
    assert_eq!(vault.max_tokens_per_customer(), 2);

    let customers = client.customers(&vault);
    let missing_err = customers
        .charge(&CustomerChargeParams::new("cus_101", 29900, Currency::SAR))
        .await
        .unwrap_err();
    assert!(matches!(missing_err, MoyasarError::InvalidTokenId));

    let tok_1 = TokenId::new("token_cus101_a").unwrap();
    let tok_2 = TokenId::new("token_cus101_b").unwrap();
    let tok_3 = TokenId::new("token_cus101_c").unwrap();
    customers
        .attach_token("cus_101", tok_1.clone())
        .await
        .unwrap();
    customers
        .attach_token("cus_101", tok_2.clone())
        .await
        .unwrap();
    assert!(customers.attach_token("cus_101", tok_3).await.is_err());

    let recent = Utc::now().to_rfc3339();
    Mock::given(method("POST"))
        .and(path("/v1/payments"))
        .and(body_partial_json(json!({
            "amount": 29900,
            "currency": "SAR",
            "metadata": { "customer_id": "cus_101", "tier": "enterprise" },
            "source": { "type": "token", "token": "token_cus101_a", "3ds": false }
        })))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(sample_module8_payment_json(
                "pay_cus_1",
                "paid",
                0,
                &recent,
            )),
        )
        .expect(1)
        .mount(&server)
        .await;

    let mut meta = Metadata::new();
    meta.insert("tier", "enterprise").unwrap();
    let charge_params = CustomerChargeParams::new("cus_101", 29900, Currency::SAR).metadata(meta);
    let charged = customers.charge(&charge_params).await.unwrap();
    assert_eq!(charged.id, "pay_cus_1");

    vault.deactivate(&tok_1).await.unwrap();
    assert_eq!(vault.resolve_tokens("cus_101").await.unwrap(), vec![tok_2]);
}

#[tokio::test]
async fn stripe_compat_subscriptions_charge_cycle_and_inactive_token_guard() {
    let (server, client) = common::start_secret_client().await;
    let active_tok = TokenId::new("token_sub_active").unwrap();
    let inactive_tok = TokenId::new("token_sub_inactive").unwrap();
    let recent = Utc::now().to_rfc3339();

    common::mount_json_endpoint(
        &server,
        ("GET", "/v1/tokens/token_sub_active"),
        200,
        sample_token_json("token_sub_active", "active", &recent),
    )
    .await;
    common::mount_json_endpoint(
        &server,
        ("GET", "/v1/tokens/token_sub_inactive"),
        200,
        sample_token_json("token_sub_inactive", "inactive", &recent),
    )
    .await;

    let expected_given_id = derive_subscription_given_id("sub_pro_9921", 4);
    Mock::given(method("POST"))
        .and(path("/v1/payments"))
        .and(body_partial_json(json!({
            "given_id": expected_given_id,
            "source": { "type": "token", "token": "token_sub_active", "3ds": false }
        })))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(sample_module8_payment_json(
                "pay_sub_4",
                "paid",
                0,
                &recent,
            )),
        )
        .expect(2)
        .mount(&server)
        .await;

    let sub = Subscription {
        id: "sub_pro_9921".to_string(),
        customer_id: "cus_101".to_string(),
        token_id: active_tok,
        current_cycle: 4,
    };
    let start = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
    let cycle = SubscriptionCycle {
        id: SubscriptionCycleId::new("sub_pro_9921", 4),
        cycle_index: 4,
        amount: 29900,
        currency: Currency::SAR,
        payment_id: None,
        period_start: start,
        period_end: start + ChronoDuration::days(30),
    };
    let params = SubscriptionCycleChargeParams::from_cycle(&sub, &cycle);
    let p1 = client.subscriptions().charge_cycle(params).await.unwrap();
    let p2 = client.subscriptions().charge_cycle(&params).await.unwrap();
    assert_eq!(p1.id, "pay_sub_4");
    assert_eq!(p2.id, "pay_sub_4");

    let bad_params = SubscriptionCycleChargeParams::new("sub_pro_9921", 4, 29900, &inactive_tok);
    assert!(
        client
            .subscriptions()
            .charge_cycle(bad_params)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn stripe_compat_subscriptions_refund_cycle_void_pro_rata_and_plan_intervals() {
    let (server, client) = common::start_secret_client().await;
    let recent = (Utc::now() - ChronoDuration::minutes(15)).to_rfc3339();
    let pay_id = "pay_cycle_04";
    let get_path = format!("/v1/payments/{pay_id}");
    let void_path = format!("/v1/payments/{pay_id}/void");
    let refund_path = format!("/v1/payments/{pay_id}/refund");

    common::mount_json_endpoint(
        &server,
        ("GET", &get_path),
        200,
        sample_module8_payment_json(pay_id, "paid", 0, &recent),
    )
    .await;
    common::mount_json_endpoint(
        &server,
        ("POST", &void_path),
        200,
        sample_module8_payment_json(pay_id, "voided", 0, &recent),
    )
    .await;
    Mock::given(method("POST"))
        .and(path(&refund_path))
        .and(body_json(json!({ "amount": 19934 })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(sample_module8_payment_json(
                pay_id, "refunded", 19934, &recent,
            )),
        )
        .expect(1)
        .mount(&server)
        .await;

    let plan = SubscriptionPlan::new("plan_pro", 29900, Currency::SAR, PlanInterval::Month);
    let start = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
    let (p_start, p_end) = plan.next_period_bounds(start);
    assert_eq!(p_end, start + ChronoDuration::days(30));

    let full_params = SubscriptionCycleRefundParams {
        subscription_id: "sub_99".to_string(),
        cycle_payment_id: pay_id.to_string(),
        period_start: p_start,
        period_end: p_end,
        strategy: RefundStrategy::Full,
    };
    let full_res = client
        .subscriptions()
        .refund_cycle(&full_params)
        .await
        .unwrap();
    assert_eq!(full_res.action, ReversalAction::Voided);

    let pro_rata_params = SubscriptionCycleRefundParams {
        strategy: RefundStrategy::ProRata {
            canceled_at: p_start + ChronoDuration::days(10),
        },
        ..full_params
    };
    let pro_rata_res = client
        .subscriptions()
        .refund_cycle(&pro_rata_params)
        .await
        .unwrap();
    assert_eq!(pro_rata_res.action, ReversalAction::Refunded);
    assert_eq!(pro_rata_res.amount_reversed, 19934);
}

#[tokio::test]
async fn unified_payment_gateway_mock_server_and_webhook_hmac_verification() {
    let mock = MoyasarMockServer::start().await;
    mock.mount_default_routes().await;

    let client = MoyasarClient::builder(common::test_secret_key_ref())
        .base_url(mock.uri())
        .webhook_secret("whsec_unified_secret_99")
        .build()
        .unwrap();
    assert_unified_capabilities(&client);

    let intent = client
        .create_intent(
            UnifiedIntentRequest::new(15000, "SAR", "https://merchant.example/cb")
                .description("Unified Order"),
        )
        .await
        .unwrap();
    assert_eq!(intent.id, "pi_mock_01");

    let charge = client
        .charge_saved_token(UnifiedTokenChargeRequest::new(
            15000,
            "SAR",
            "token_mock_01",
            Uuid::new_v4(),
        ))
        .await
        .unwrap();
    assert_eq!(charge.transaction_id, "pay_mock_01");

    let rev = client
        .refund_or_void("pay_mock_01", Some(5000))
        .await
        .unwrap();
    assert_eq!(rev.action, "refunded");
    assert_eq!(rev.amount_reversed_minor, 5000);

    let now_str = Utc::now().to_rfc3339();
    let raw_event = webhook_event_fixture_at("payment_paid", "whsec_unified_secret_99", &now_str);
    let raw_bytes = serde_json::to_vec(&raw_event).unwrap();
    let sig = compute_webhook_signature(&raw_bytes, "whsec_unified_secret_99");

    let mut headers = http::HeaderMap::new();
    headers.insert(
        WEBHOOK_SIGNATURE_HEADER,
        format!("sha256={sig}").parse().unwrap(),
    );
    assert_eq!(
        client
            .verify_webhook(&headers, &raw_bytes)
            .unwrap()
            .event_type,
        "payment_paid"
    );

    let mut token_headers = http::HeaderMap::new();
    token_headers.insert(
        WEBHOOK_SECRET_TOKEN_HEADER,
        "whsec_unified_secret_99".parse().unwrap(),
    );
    assert!(matches!(
        client.verify_webhook(&token_headers, &raw_bytes),
        Err(MoyasarError::InvalidWebhookSignature)
    ));
    let opt_in = client
        .verify_webhook_with_secret_token(&token_headers, &raw_bytes)
        .unwrap();
    assert_eq!(opt_in.event_type, "payment_paid");
}

#[test]
fn test_utils_deterministic_fixtures_cover_all_12_events_stcpay_sadad_and_iso8583() {
    let events = all_webhook_event_fixtures("whsec_fixture_secret");
    assert_eq!(events.len(), 12);
    for (name, payload) in events {
        let evt: WebhookEvent = serde_json::from_value(payload).unwrap();
        if name.starts_with("card_auth_") {
            assert!(evt.into_card_auth().is_ok());
        } else {
            assert!(evt.into_payment().is_ok());
        }
    }

    let stc_ok: Payment =
        serde_json::from_value(stcpay_sandbox_fixture(STCPAY_SANDBOX_SUCCESS)).unwrap();
    let stc_rej: Payment =
        serde_json::from_value(stcpay_sandbox_fixture(STCPAY_SANDBOX_REJECTED)).unwrap();
    let stc_ins: Payment =
        serde_json::from_value(stcpay_sandbox_fixture(STCPAY_SANDBOX_INSUFFICIENT_FUNDS)).unwrap();
    assert_eq!(stc_ok.status, PaymentStatus::Paid);
    assert_eq!(stc_rej.status, PaymentStatus::Failed);
    assert_eq!(stc_ins.status, PaymentStatus::Failed);

    let sadad: Payment = serde_json::from_value(sadad_sandbox_fixture()).unwrap();
    assert_eq!(sadad.status, PaymentStatus::Initiated);

    let iso_err: ApiErrorPayload =
        serde_json::from_value(iso8583_error_fixture("51", "Insufficient funds")).unwrap();
    assert_eq!(iso_err.response_code, Some(ResponseCode::InsufficientFunds));
}

#[tokio::test]
async fn deduplicator_bounded_eviction_and_verifier_missing_env_secret_guard() {
    let ttl = std::time::Duration::from_secs(60);
    let dedup = InMemoryDeduplicator::with_capacity_limit(ttl, 3);
    let id1 = Uuid::from_u128(101);
    let id2 = Uuid::from_u128(102);
    let id3 = Uuid::from_u128(103);
    let id4 = Uuid::from_u128(104);

    assert!(dedup.check_and_record(id1).await.unwrap());
    assert!(dedup.check_and_record(id2).await.unwrap());
    assert!(dedup.check_and_record(id3).await.unwrap());
    assert_eq!(dedup.len(), 3);

    assert!(dedup.check_and_record(id4).await.unwrap());
    assert_eq!(dedup.len(), 3);
    assert!(!dedup.check_and_record(id4).await.unwrap());
    assert!(dedup.check_and_record(id1).await.unwrap());

    let secret_ref = "$MISSING_MOYASAR_WEBHOOK_SECRET_XYZ";
    let res = ConstantTimeSecretVerifier::from_secret(secret_ref);
    assert!(matches!(res, Err(MoyasarError::InvalidWebhookSecret)));
}
