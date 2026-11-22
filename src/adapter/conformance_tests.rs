//! Offline `wiremock` conformance test suite for `MoyasarProvider` (US-9.2.0 Unit 4).

use std::collections::BTreeMap;

use serde_json::{Value, json};
use some_payment_core::{
    CaptureMode, CaptureRequest, CheckoutSessionRequest, CreatePaymentRequest, HttpMethod, Money,
    NextAction, PaymentCapabilities, PaymentError, PaymentProvider, PaymentStatus, ProviderId,
    ProviderReference, RawWebhookRequest, RefundRequest, RefundStatus, VoidRequest,
};
use wiremock::matchers::{body_json, body_partial_json, body_string, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::moyasar_provider::{MoyasarProvider, MoyasarProviderConfig};

const PAYMENT_UUID: &str = "550e8400-e29b-41d4-a716-446655440000";
const GIVEN_UUID: &str = "f47ac10b-58cc-4372-a567-0e02b2c3d479";
const WEBHOOK_CANARY: &str = "wh_secret_canary_999";

fn make_provider_with_key(server: &MockServer, api_key: &str) -> MoyasarProvider {
    let config = MoyasarProviderConfig::new(api_key)
        .expect("valid api key")
        .with_webhook_secret(WEBHOOK_CANARY)
        .with_base_url(server.uri());
    MoyasarProvider::new(config).expect("valid provider")
}

fn make_provider(server: &MockServer) -> MoyasarProvider {
    make_provider_with_key(server, "sk_test_conformance_key")
}

fn sample_reference() -> ProviderReference {
    ProviderReference::builder(ProviderId::new("moyasar"))
        .payment_id(PAYMENT_UUID)
        .order_id("ORD-REF-1")
        .build()
        .expect("valid reference")
}

fn payment_json(status: &str, amount: u64, source: Value) -> Value {
    json!({
        "id": PAYMENT_UUID,
        "status": status,
        "amount": amount,
        "currency": "SAR",
        "source": source,
    })
}

fn base_create_request(amount: u64, order_id: &str) -> CreatePaymentRequest {
    let mut req = CreatePaymentRequest::new(Money::new(amount, "SAR").expect("money"), order_id);
    req.payment_token = Some("tok_test_conformance".to_owned());
    req
}

fn base_checkout_request(amount: u64, order_id: &str) -> CheckoutSessionRequest {
    CheckoutSessionRequest {
        provider_id: None,
        money: Money::new(amount, "SAR").expect("money"),
        order_id: order_id.to_owned(),
        success_url: "https://merchant.example/success".to_owned(),
        cancel_url: Some("https://merchant.example/cancel".to_owned()),
        description: Some("Order checkout".to_owned()),
        customer_email: None,
        customer_phone: None,
        capture_mode: CaptureMode::Automatic,
        idempotency_key: None,
        metadata: BTreeMap::new(),
    }
}

fn raw_webhook(payload: &Value) -> RawWebhookRequest {
    RawWebhookRequest {
        raw_body: payload.to_string().into_bytes(),
        headers: BTreeMap::new(),
        query_params: BTreeMap::new(),
        method: HttpMethod::Post,
    }
}

async fn mount_payment_post(server: &MockServer, response_body: Value) {
    Mock::given(method("POST"))
        .and(path("/payments"))
        .respond_with(ResponseTemplate::new(201).set_body_json(response_body))
        .expect(1)
        .mount(server)
        .await;
}

#[test]
fn tc_capabilities_identity_and_livemode() {
    let test_provider = MoyasarProvider::from_api_key("sk_test_cap_01").expect("valid test key");
    let live_provider = MoyasarProvider::from_api_key("sk_live_cap_01").expect("valid live key");
    let pk_test = MoyasarProvider::from_api_key("pk_test_cap_01").expect("valid pk test key");
    let pk_live = MoyasarProvider::from_api_key("pk_live_cap_01").expect("valid pk live key");

    assert_eq!(test_provider.id().as_str(), "moyasar");
    assert!(!test_provider.livemode());
    assert!(live_provider.livemode());
    assert!(!pk_test.livemode());
    assert!(pk_live.livemode());

    let caps = test_provider.capabilities();
    assert!(caps.contains(PaymentCapabilities::NATIVE_CREATE_IDEMPOTENCY));
    assert!(!caps.contains(PaymentCapabilities::NATIVE_MUTATION_IDEMPOTENCY));
    assert!(!caps.contains(PaymentCapabilities::VOID_SETTLED_WINDOW));
    assert!(caps.contains(PaymentCapabilities::WEBHOOK_VERIFICATION_HANDSHAKE));
}

#[tokio::test]
async fn tc_create_payment_paid_sar_halalas_forwards_given_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/payments"))
        .and(body_partial_json(json!({
            "amount": 10_000,
            "currency": "SAR",
            "given_id": GIVEN_UUID,
        })))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(payment_json("paid", 10_000, json!({ "type": "token" }))),
        )
        .expect(1)
        .mount(&server)
        .await;

    let provider = make_provider(&server);
    let mut req = base_create_request(10_000, "ORD-PAID-1");
    req.idempotency_key = Some(GIVEN_UUID.to_owned());

    let res = provider.create_payment(&req).await.expect("paid payment");
    assert_eq!(res.status, PaymentStatus::Paid);
    assert_eq!(res.money, Money::new(10_000, "SAR").expect("money"));
    assert_eq!(res.reference.payment_id(), Some(PAYMENT_UUID));
    assert!(!res.livemode);
}

#[tokio::test]
async fn tc_create_payment_http_201_failed_quirk() {
    let server = MockServer::start().await;
    mount_payment_post(
        &server,
        payment_json("failed", 10_000, json!({ "type": "token" })),
    )
    .await;

    let provider = make_provider(&server);
    let req = base_create_request(10_000, "ORD-FAIL-201");

    let res = provider
        .create_payment(&req)
        .await
        .expect("http 201 failed body parses");
    assert_eq!(res.status, PaymentStatus::Failed);
    assert_ne!(res.status, PaymentStatus::Paid);
}

#[tokio::test]
async fn tc_create_payment_initiated_stcpay_maps_to_otp_challenge() {
    let server = MockServer::start().await;
    let otp_url = "https://api.moyasar.com/v1/stc_pays/otp/submit";
    mount_payment_post(
        &server,
        payment_json(
            "initiated",
            10_000,
            json!({ "type": "stcpay", "transaction_url": otp_url }),
        ),
    )
    .await;

    let provider = make_provider(&server);
    let mut req = base_create_request(10_000, "ORD-STCPAY-1");
    req.metadata
        .insert("source_type".to_owned(), "stcpay".to_owned());

    let res = provider
        .create_payment(&req)
        .await
        .expect("stcpay initiated");
    assert_eq!(res.status, PaymentStatus::RequiresAction);
    assert_eq!(
        res.next_action,
        Some(NextAction::OtpChallenge {
            submission_url: otp_url.to_owned(),
        })
    );
    assert!(!matches!(res.next_action, Some(NextAction::Redirect { .. })));
}

#[tokio::test]
async fn tc_create_payment_initiated_token_maps_to_redirect() {
    let server = MockServer::start().await;
    let redirect_url = "https://api.moyasar.com/v1/3ds/token-challenge";
    mount_payment_post(
        &server,
        payment_json(
            "initiated",
            10_000,
            json!({ "type": "token", "transaction_url": redirect_url }),
        ),
    )
    .await;

    let provider = make_provider(&server);
    let req = base_create_request(10_000, "ORD-TOKEN-3DS");

    let res = provider
        .create_payment(&req)
        .await
        .expect("token initiated");
    assert_eq!(res.status, PaymentStatus::RequiresAction);
    assert_eq!(
        res.next_action,
        Some(NextAction::Redirect {
            url: redirect_url.to_owned(),
        })
    );
}

#[tokio::test]
async fn tc_create_payment_initiated_creditcard_maps_to_redirect() {
    let server = MockServer::start().await;
    let redirect_url = "https://api.moyasar.com/v1/3ds/card-challenge";
    mount_payment_post(
        &server,
        payment_json(
            "initiated",
            10_000,
            json!({ "type": "creditcard", "transaction_url": redirect_url }),
        ),
    )
    .await;

    let config = MoyasarProviderConfig::new("sk_test_conformance_key")
        .expect("valid api key")
        .with_base_url(server.uri())
        .with_allow_raw_cards(true);
    let provider = MoyasarProvider::new(config).expect("valid provider");
    let mut req = base_create_request(10_000, "ORD-CC-3DS");
    req.metadata
        .insert("source_type".to_owned(), "creditcard".to_owned());

    let res = provider
        .create_payment(&req)
        .await
        .expect("creditcard initiated");
    assert_eq!(res.status, PaymentStatus::RequiresAction);
    assert_eq!(
        res.next_action,
        Some(NextAction::Redirect {
            url: redirect_url.to_owned(),
        })
    );
}

#[tokio::test]
async fn tc_create_payment_initiated_sadadbill_maps_to_offline_bill() {
    let server = MockServer::start().await;
    let instructions_url = "https://moyasar.com/sadad/bills/SADAD-987654";
    mount_payment_post(
        &server,
        payment_json(
            "initiated",
            10_000,
            json!({
                "type": "sadadbill",
                "bill_reference": "SADAD-987654",
                "transaction_url": instructions_url,
            }),
        ),
    )
    .await;

    let provider = make_provider(&server);
    let mut req = base_create_request(10_000, "ORD-SADAD-1");
    req.metadata
        .insert("source_type".to_owned(), "sadadbill".to_owned());

    let res = provider
        .create_payment(&req)
        .await
        .expect("sadadbill initiated");
    assert_eq!(res.status, PaymentStatus::RequiresAction);
    assert_eq!(
        res.next_action,
        Some(NextAction::OfflineBill {
            bill_reference: "SADAD-987654".to_owned(),
            instructions_url: Some(instructions_url.to_owned()),
        })
    );
}

#[tokio::test]
async fn tc_create_payment_verified_maps_to_setup_completed() {
    let server = MockServer::start().await;
    mount_payment_post(
        &server,
        payment_json("verified", 100, json!({ "type": "token" })),
    )
    .await;

    let provider = make_provider(&server);
    let req = base_create_request(100, "ORD-SETUP-1");

    let res = provider
        .create_payment(&req)
        .await
        .expect("verified setup");
    assert_eq!(res.status, PaymentStatus::SetupCompleted);
    assert!(res.next_action.is_none());
}

#[tokio::test]
async fn tc_error_classification_3ds_auth_error_is_three_ds_failed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/payments"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "type": "authentication_error",
            "code": "3ds_auth_error",
            "message": "3-D Secure authentication failed",
        })))
        .expect(1)
        .mount(&server)
        .await;

    let provider = make_provider(&server);
    let req = base_create_request(10_000, "ORD-3DS-ERR");

    let err = provider
        .create_payment(&req)
        .await
        .expect_err("3ds_auth_error must fail");
    assert!(matches!(err, PaymentError::ThreeDsFailed { .. }));
    assert!(!matches!(err, PaymentError::AuthenticationError { .. }));
}

#[tokio::test]
async fn tc_create_checkout_posts_invoice_and_returns_redirect() {
    let server = MockServer::start().await;
    let invoice_url = "https://moyasar.com/i/inv_conformance_01";
    Mock::given(method("POST"))
        .and(path("/invoices"))
        .and(body_partial_json(json!({
            "amount": 25_000,
            "currency": "SAR",
            "success_url": "https://merchant.example/success",
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": "inv_conformance_01",
            "amount": 25_000,
            "currency": "SAR",
            "url": invoice_url,
        })))
        .expect(1)
        .mount(&server)
        .await;

    let provider = make_provider(&server);
    let req = base_checkout_request(25_000, "ORD-CHK-1");

    let res = provider
        .create_checkout(&req)
        .await
        .expect("hosted checkout");
    assert_eq!(res.status, PaymentStatus::RequiresAction);
    assert_eq!(res.checkout_url.as_deref(), Some(invoice_url));
    assert_eq!(
        res.next_action,
        Some(NextAction::Redirect {
            url: invoice_url.to_owned(),
        })
    );
    assert_eq!(
        res.reference.checkout_session_id(),
        Some("inv_conformance_01")
    );
}

#[tokio::test]
async fn tc_capture_full_sends_empty_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/payments/{PAYMENT_UUID}/capture")))
        .and(body_string(""))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(payment_json("captured", 10_000, json!({ "type": "token" }))),
        )
        .expect(1)
        .mount(&server)
        .await;

    let provider = make_provider(&server);
    let req = CaptureRequest {
        reference: sample_reference(),
        amount: None,
        idempotency_key: None,
    };

    let res = provider.capture(&req).await.expect("full capture");
    assert_eq!(res.status, PaymentStatus::Captured);
}

#[tokio::test]
async fn tc_capture_partial_sends_amount_json() {
    let server = MockServer::start().await;
    let mut body = payment_json("captured", 10_000, json!({ "type": "token" }));
    body["captured"] = json!(4_000);
    Mock::given(method("POST"))
        .and(path(format!("/payments/{PAYMENT_UUID}/capture")))
        .and(body_json(json!({ "amount": 4_000 })))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;

    let provider = make_provider(&server);
    let req = CaptureRequest {
        reference: sample_reference(),
        amount: Some(Money::new(4_000, "SAR").expect("money")),
        idempotency_key: None,
    };

    let res = provider.capture(&req).await.expect("partial capture");
    assert_eq!(res.status, PaymentStatus::Captured);
    assert_eq!(res.captured_amount, Some(4_000));
}

#[tokio::test]
async fn tc_void_payment_sends_empty_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(format!("/payments/{PAYMENT_UUID}/void")))
        .and(body_string(""))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(payment_json("voided", 10_000, json!({ "type": "token" }))),
        )
        .expect(1)
        .mount(&server)
        .await;

    let provider = make_provider(&server);
    let req = VoidRequest {
        reference: sample_reference(),
        reason: Some("customer_canceled".to_owned()),
        idempotency_key: None,
    };

    let res = provider.void_payment(&req).await.expect("void payment");
    assert_eq!(res.status, PaymentStatus::Voided);
}

#[tokio::test]
async fn tc_refund_full_with_reason_sends_empty_body() {
    let server = MockServer::start().await;
    let mut body = payment_json("refunded", 10_000, json!({ "type": "token" }));
    body["refunded"] = json!(10_000);
    Mock::given(method("POST"))
        .and(path(format!("/payments/{PAYMENT_UUID}/refund")))
        .and(body_string(""))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;

    let provider = make_provider(&server);
    let req = RefundRequest {
        reference: sample_reference(),
        amount: None,
        reason: Some("duplicate".to_owned()),
        idempotency_key: None,
    };

    let res = provider.refund(&req).await.expect("full refund");
    assert_eq!(res.status, RefundStatus::Succeeded);
    assert_eq!(res.money, Money::new(10_000, "SAR").expect("money"));
}

#[tokio::test]
async fn tc_get_payment_fetches_by_uuid() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/payments/{PAYMENT_UUID}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(payment_json("paid", 10_000, json!({ "type": "token" }))),
        )
        .expect(1)
        .mount(&server)
        .await;

    let provider = make_provider(&server);
    let res = provider
        .get_payment(&sample_reference())
        .await
        .expect("get payment");
    assert_eq!(res.status, PaymentStatus::Paid);
    assert_eq!(res.reference.payment_id(), Some(PAYMENT_UUID));
    assert_eq!(res.money, Money::new(10_000, "SAR").expect("money"));
}

#[tokio::test]
async fn tc_publishable_key_fails_closed_locally_with_zero_http_calls() {
    let server = MockServer::start().await;
    let provider = make_provider_with_key(&server, "pk_test_local_guard");
    let reference = sample_reference();

    let cap_req = CaptureRequest {
        reference: reference.clone(),
        amount: None,
        idempotency_key: None,
    };
    let void_req = VoidRequest {
        reference: reference.clone(),
        reason: None,
        idempotency_key: None,
    };
    let refund_req = RefundRequest {
        reference: reference.clone(),
        amount: None,
        reason: None,
        idempotency_key: None,
    };
    let checkout_req = base_checkout_request(5_000, "ORD-PK-GUARD");

    assert!(matches!(
        provider.capture(&cap_req).await.unwrap_err(),
        PaymentError::AuthenticationError { .. }
    ));
    assert!(matches!(
        provider.void_payment(&void_req).await.unwrap_err(),
        PaymentError::AuthenticationError { .. }
    ));
    assert!(matches!(
        provider.refund(&refund_req).await.unwrap_err(),
        PaymentError::AuthenticationError { .. }
    ));
    assert!(matches!(
        provider.create_checkout(&checkout_req).await.unwrap_err(),
        PaymentError::AuthenticationError { .. }
    ));
    assert!(matches!(
        provider.get_payment(&reference).await.unwrap_err(),
        PaymentError::AuthenticationError { .. }
    ));
    assert!(server.received_requests().await.expect("requests").is_empty());
}

#[tokio::test]
async fn tc_webhook_valid_live_scrubs_canary_secret() {
    let server = MockServer::start().await;
    let provider = make_provider(&server);
    let payload = json!({
        "id": "evt_live_paid_1",
        "type": "payment_paid",
        "live": true,
        "secret_token": WEBHOOK_CANARY,
        "data": { "id": PAYMENT_UUID, "amount": 10_000, "currency": "SAR" },
    });

    let event = provider
        .parse_and_verify_webhook(&raw_webhook(&payload))
        .await
        .expect("verified live webhook");
    assert!(event.livemode);
    assert_eq!(event.status, Some(PaymentStatus::Paid));
    assert!(!event.raw_redacted.to_string().contains(WEBHOOK_CANARY));
}

#[tokio::test]
async fn tc_webhook_wrong_secret_fails_verification() {
    let server = MockServer::start().await;
    let provider = make_provider(&server);
    let payload = json!({
        "id": "evt_bad_secret_1",
        "type": "payment_paid",
        "live": false,
        "secret_token": "wh_wrong_secret_000",
        "data": { "id": PAYMENT_UUID, "amount": 10_000, "currency": "SAR" },
    });

    let err = provider
        .parse_and_verify_webhook(&raw_webhook(&payload))
        .await
        .expect_err("wrong secret must fail");
    assert!(matches!(err, PaymentError::WebhookVerificationFailed { .. }));
}

#[tokio::test]
async fn tc_webhook_payment_faild_typo_maps_to_failed() {
    let server = MockServer::start().await;
    let provider = make_provider(&server);
    let payload = json!({
        "id": "evt_faild_typo_1",
        "type": "payment_faild",
        "live": false,
        "secret_token": WEBHOOK_CANARY,
        "data": { "id": PAYMENT_UUID, "amount": 5_000, "currency": "SAR" },
    });

    let event = provider
        .parse_and_verify_webhook(&raw_webhook(&payload))
        .await
        .expect("verified typo webhook");
    assert_eq!(event.status, Some(PaymentStatus::Failed));
    assert!(!event.raw_redacted.to_string().contains(WEBHOOK_CANARY));
}

#[tokio::test]
async fn tc_webhook_verification_handshake_redacts_debug() {
    let server = MockServer::start().await;
    let provider = make_provider(&server);
    let payload = json!({
        "id": "evt_handshake_1",
        "type": "webhook_verification",
        "live": true,
        "secret_token": WEBHOOK_CANARY,
        "data": { "webhook_id": "wh_handshake_01", "verification_code": "654321" },
    });

    let event = provider
        .parse_and_verify_webhook(&raw_webhook(&payload))
        .await
        .expect("verified handshake webhook");
    let challenge = event
        .verification_challenge
        .expect("verification challenge populated");
    assert_eq!(challenge.webhook_id, "wh_handshake_01");
    assert_eq!(challenge.verification_code, "654321");
    let dbg = format!("{challenge:?}");
    assert!(dbg.contains("[REDACTED]"));
    assert!(!dbg.contains("654321"));
    assert!(!event.raw_redacted.to_string().contains(WEBHOOK_CANARY));
}
