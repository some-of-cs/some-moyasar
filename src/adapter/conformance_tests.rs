//! Offline `wiremock` conformance test suite for `MoyasarProvider` (US-9.2.0 Unit 4).

use std::collections::BTreeMap;

use serde_json::{Value, json};
use some_payment_core::{
    CaptureMode, CaptureRequest, CheckoutSessionRequest, CreatePaymentRequest, HttpMethod, Money,
    NextAction, PaymentCapabilities, PaymentError, PaymentProvider, PaymentResult, PaymentStatus,
    ProviderId, ProviderReference, RawWebhookRequest, RefundRequest, RefundStatus, VoidRequest,
};
use wiremock::matchers::{body_json, body_partial_json, body_string, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::moyasar_provider::{MoyasarProvider, MoyasarProviderConfig};

const PAYMENT_UUID: &str = "550e8400-e29b-41d4-a716-446655440000";
const GIVEN_UUID: &str = "f47ac10b-58cc-4372-a567-0e02b2c3d479";
const WEBHOOK_CANARY: &str = "wh_secret_canary_999";

fn server_provider(api_key: &str, base_url: &str) -> MoyasarProvider {
    let config = MoyasarProviderConfig::new(api_key)
        .expect("valid api key")
        .with_webhook_secret(WEBHOOK_CANARY)
        .with_base_url(base_url);
    MoyasarProvider::new(config).expect("valid provider")
}

fn raw_card_provider(base_url: &str) -> MoyasarProvider {
    let config = MoyasarProviderConfig::new("sk_test_conformance_key")
        .expect("valid api key")
        .with_base_url(base_url)
        .with_allow_raw_cards(true);
    MoyasarProvider::new(config).expect("valid provider")
}

fn webhook_provider() -> MoyasarProvider {
    let config = MoyasarProviderConfig::new("sk_test_conformance_key")
        .expect("valid api key")
        .with_webhook_secret(WEBHOOK_CANARY);
    MoyasarProvider::new(config).expect("valid provider")
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

fn raw_webhook_from(payload: &Value) -> RawWebhookRequest {
    RawWebhookRequest {
        raw_body: payload.to_string().into_bytes(),
        headers: BTreeMap::new(),
        query_params: BTreeMap::new(),
        method: HttpMethod::Post,
    }
}

fn webhook_request(event_type: &str, live: bool, data: Value) -> RawWebhookRequest {
    let payload = json!({
        "id": format!("evt_{event_type}"),
        "type": event_type,
        "live": live,
        "secret_token": WEBHOOK_CANARY,
        "data": data,
    });
    raw_webhook_from(&payload)
}

async fn run_create_payment_case(
    source_type: Option<&str>,
    allow_raw_cards: bool,
    response_body: Value,
) -> PaymentResult {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/payments"))
        .respond_with(ResponseTemplate::new(201).set_body_json(response_body))
        .expect(1)
        .mount(&server)
        .await;

    let provider = if allow_raw_cards {
        raw_card_provider(&server.uri())
    } else {
        server_provider("sk_test_conformance_key", &server.uri())
    };
    let mut req = CreatePaymentRequest::new(Money::new(10_000, "SAR").expect("money"), "ORD-CASE");
    req.payment_token = Some("tok_test_conformance".to_owned());
    if let Some(st) = source_type {
        req.metadata.insert("source_type".to_owned(), st.to_owned());
    }
    provider.create_payment(&req).await.expect("create_payment")
}

async fn mount_empty_post(server: &MockServer, endpoint: &str, response_body: Value) {
    Mock::given(method("POST"))
        .and(path(endpoint))
        .and(body_string(""))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
        .expect(1)
        .mount(server)
        .await;
}

#[test]
fn tc_identity_and_livemode_derived_from_key_prefix() {
    let sk_test = MoyasarProvider::from_api_key("sk_test_cap_01").expect("valid test key");
    let sk_live = MoyasarProvider::from_api_key("sk_live_cap_01").expect("valid live key");
    let pk_test = MoyasarProvider::from_api_key("pk_test_cap_01").expect("valid pk test key");
    let pk_live = MoyasarProvider::from_api_key("pk_live_cap_01").expect("valid pk live key");

    assert_eq!(sk_test.id().as_str(), "moyasar");
    assert_eq!(
        [
            sk_test.livemode(),
            sk_live.livemode(),
            pk_test.livemode(),
            pk_live.livemode(),
        ],
        [false, true, false, true]
    );
}

#[test]
fn tc_capabilities_match_moyasar_contract() {
    let provider = MoyasarProvider::from_api_key("sk_test_cap_02").expect("valid test key");
    let caps = provider.capabilities();
    let required = PaymentCapabilities::NATIVE_CREATE_IDEMPOTENCY
        | PaymentCapabilities::WEBHOOK_VERIFICATION_HANDSHAKE;
    let forbidden = PaymentCapabilities::NATIVE_MUTATION_IDEMPOTENCY
        | PaymentCapabilities::VOID_SETTLED_WINDOW;

    assert!(caps.contains(required));
    assert!(!caps.intersects(forbidden));
}

#[tokio::test]
async fn tc_create_payment_paid_sar_halalas_forwards_given_id() {
    let server = MockServer::start().await;
    let resp = payment_json("paid", 10_000, json!({ "type": "token" }));
    Mock::given(method("POST"))
        .and(path("/payments"))
        .and(body_partial_json(json!({
            "amount": 10_000,
            "currency": "SAR",
            "given_id": GIVEN_UUID,
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(resp))
        .expect(1)
        .mount(&server)
        .await;

    let provider = server_provider("sk_test_conformance_key", &server.uri());
    let mut req = CreatePaymentRequest::new(Money::new(10_000, "SAR").expect("money"), "ORD-PAID");
    req.payment_token = Some("tok_test_conformance".to_owned());
    req.idempotency_key = Some(GIVEN_UUID.to_owned());

    let res = provider.create_payment(&req).await.expect("paid payment");
    assert_eq!(
        (res.status, res.reference.payment_id(), res.livemode),
        (PaymentStatus::Paid, Some(PAYMENT_UUID), false)
    );
    assert_eq!(res.money, Money::new(10_000, "SAR").expect("money"));
}

#[tokio::test]
async fn tc_create_payment_http_201_failed_quirk() {
    let body = payment_json("failed", 10_000, json!({ "type": "token" }));
    let res = run_create_payment_case(None, false, body).await;
    assert_eq!(res.status, PaymentStatus::Failed);
    assert_ne!(res.status, PaymentStatus::Paid);
}

#[tokio::test]
async fn tc_create_payment_initiated_stcpay_maps_to_otp_challenge() {
    let otp_url = "https://api.moyasar.com/v1/stc_pays/otp/submit";
    let body = payment_json(
        "initiated",
        10_000,
        json!({ "type": "stcpay", "transaction_url": otp_url }),
    );
    let res = run_create_payment_case(Some("stcpay"), false, body).await;
    assert_eq!(res.status, PaymentStatus::RequiresAction);
    assert_eq!(
        res.next_action,
        Some(NextAction::OtpChallenge {
            submission_url: otp_url.to_owned(),
        })
    );
    assert!(!matches!(
        res.next_action,
        Some(NextAction::Redirect { .. })
    ));
}

#[tokio::test]
async fn tc_create_payment_initiated_token_and_creditcard_map_to_redirect() {
    let cases = [
        (None, false, "token", "https://api.moyasar.com/v1/3ds/token"),
        (
            Some("creditcard"),
            true,
            "creditcard",
            "https://api.moyasar.com/v1/3ds/card",
        ),
    ];
    for (req_source, allow_raw, resp_source, url) in cases {
        let body = payment_json(
            "initiated",
            10_000,
            json!({ "type": resp_source, "transaction_url": url }),
        );
        let res = run_create_payment_case(req_source, allow_raw, body).await;
        assert_eq!(res.status, PaymentStatus::RequiresAction);
        assert_eq!(
            res.next_action,
            Some(NextAction::Redirect {
                url: url.to_owned(),
            })
        );
    }
}

#[tokio::test]
async fn tc_create_payment_initiated_sadadbill_maps_to_offline_bill() {
    let instructions_url = "https://moyasar.com/sadad/bills/SADAD-987654";
    let body = payment_json(
        "initiated",
        10_000,
        json!({
            "type": "sadadbill",
            "bill_reference": "SADAD-987654",
            "transaction_url": instructions_url,
        }),
    );
    let res = run_create_payment_case(Some("sadadbill"), false, body).await;
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
    let body = payment_json("verified", 10_000, json!({ "type": "token" }));
    let res = run_create_payment_case(None, false, body).await;
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

    let provider = server_provider("sk_test_conformance_key", &server.uri());
    let mut req = CreatePaymentRequest::new(Money::new(10_000, "SAR").expect("money"), "ORD-3DS");
    req.payment_token = Some("tok_test_conformance".to_owned());

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

    let provider = server_provider("sk_test_conformance_key", &server.uri());
    let req = base_checkout_request(25_000, "ORD-CHK-1");

    let res = provider
        .create_checkout(&req)
        .await
        .expect("hosted checkout");
    assert_eq!(
        (
            res.status,
            res.checkout_url.as_deref(),
            res.reference.checkout_session_id(),
        ),
        (
            PaymentStatus::RequiresAction,
            Some(invoice_url),
            Some("inv_conformance_01"),
        )
    );
    assert_eq!(
        res.next_action,
        Some(NextAction::Redirect {
            url: invoice_url.to_owned(),
        })
    );
}

#[tokio::test]
async fn tc_capture_full_and_partial_request_bodies() {
    let server = MockServer::start().await;
    let capture_path = format!("/payments/{PAYMENT_UUID}/capture");
    let full_body = payment_json("captured", 10_000, json!({ "type": "token" }));
    mount_empty_post(&server, &capture_path, full_body).await;

    let mut partial_body = payment_json("captured", 10_000, json!({ "type": "token" }));
    partial_body["captured"] = json!(4_000);
    Mock::given(method("POST"))
        .and(path(&capture_path))
        .and(body_json(json!({ "amount": 4_000 })))
        .respond_with(ResponseTemplate::new(200).set_body_json(partial_body))
        .expect(1)
        .mount(&server)
        .await;

    let provider = server_provider("sk_test_conformance_key", &server.uri());
    let full_req = CaptureRequest {
        reference: sample_reference(),
        amount: None,
        idempotency_key: None,
    };
    let full_res = provider.capture(&full_req).await.expect("full capture");
    assert_eq!(full_res.status, PaymentStatus::Captured);

    let partial_req = CaptureRequest {
        reference: sample_reference(),
        amount: Some(Money::new(4_000, "SAR").expect("money")),
        idempotency_key: None,
    };
    let partial_res = provider
        .capture(&partial_req)
        .await
        .expect("partial capture");
    assert_eq!(partial_res.status, PaymentStatus::Captured);
    assert_eq!(partial_res.captured_amount, Some(4_000));
}

#[tokio::test]
async fn tc_void_payment_sends_empty_body() {
    let server = MockServer::start().await;
    let endpoint = format!("/payments/{PAYMENT_UUID}/void");
    let body = payment_json("voided", 10_000, json!({ "type": "token" }));
    mount_empty_post(&server, &endpoint, body).await;

    let provider = server_provider("sk_test_conformance_key", &server.uri());
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
    let endpoint = format!("/payments/{PAYMENT_UUID}/refund");
    let mut body = payment_json("refunded", 10_000, json!({ "type": "token" }));
    body["refunded"] = json!(10_000);
    mount_empty_post(&server, &endpoint, body).await;

    let provider = server_provider("sk_test_conformance_key", &server.uri());
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
    let body = payment_json("paid", 10_000, json!({ "type": "token" }));
    Mock::given(method("GET"))
        .and(path(format!("/payments/{PAYMENT_UUID}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .expect(1)
        .mount(&server)
        .await;

    let provider = server_provider("sk_test_conformance_key", &server.uri());
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
    let provider = server_provider("pk_test_local_guard", &server.uri());
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

    let errors = [
        provider.capture(&cap_req).await.unwrap_err(),
        provider.void_payment(&void_req).await.unwrap_err(),
        provider.refund(&refund_req).await.unwrap_err(),
        provider.create_checkout(&checkout_req).await.unwrap_err(),
        provider.get_payment(&reference).await.unwrap_err(),
    ];
    for err in errors {
        assert!(matches!(err, PaymentError::AuthenticationError { .. }));
    }

    let received = server.received_requests().await.expect("requests");
    assert!(received.is_empty());
}

#[tokio::test]
async fn tc_webhook_valid_live_scrubs_canary_secret() {
    let provider = webhook_provider();
    let raw = webhook_request(
        "payment_paid",
        true,
        json!({ "id": PAYMENT_UUID, "amount": 10_000, "currency": "SAR" }),
    );
    let event = provider
        .parse_and_verify_webhook(&raw)
        .await
        .expect("verified live webhook");
    assert!(event.livemode);
    assert_eq!(event.status, Some(PaymentStatus::Paid));
    assert!(!event.raw_redacted.to_string().contains(WEBHOOK_CANARY));
}

#[tokio::test]
async fn tc_webhook_wrong_secret_fails_verification() {
    let provider = webhook_provider();
    let payload = json!({
        "id": "evt_bad_secret_1",
        "type": "payment_paid",
        "live": false,
        "secret_token": "wh_wrong_secret_000",
        "data": { "id": PAYMENT_UUID, "amount": 10_000, "currency": "SAR" },
    });
    let err = provider
        .parse_and_verify_webhook(&raw_webhook_from(&payload))
        .await
        .expect_err("wrong secret must fail");
    assert!(matches!(
        err,
        PaymentError::WebhookVerificationFailed { .. }
    ));
}

#[tokio::test]
async fn tc_webhook_payment_faild_typo_maps_to_failed() {
    let provider = webhook_provider();
    let raw = webhook_request(
        "payment_faild",
        false,
        json!({ "id": PAYMENT_UUID, "amount": 5_000, "currency": "SAR" }),
    );
    let event = provider
        .parse_and_verify_webhook(&raw)
        .await
        .expect("verified typo webhook");
    assert_eq!(event.status, Some(PaymentStatus::Failed));
    assert!(!event.raw_redacted.to_string().contains(WEBHOOK_CANARY));
}

#[tokio::test]
async fn tc_webhook_verification_handshake_redacts_debug() {
    let provider = webhook_provider();
    let raw = webhook_request(
        "webhook_verification",
        true,
        json!({ "webhook_id": "wh_handshake_01", "verification_code": "654321" }),
    );
    let event = provider
        .parse_and_verify_webhook(&raw)
        .await
        .expect("verified handshake webhook");
    let challenge = event
        .verification_challenge
        .expect("verification challenge populated");
    assert_eq!(
        (
            challenge.webhook_id.as_str(),
            challenge.verification_code.as_str(),
        ),
        ("wh_handshake_01", "654321")
    );
    let dbg = format!("{challenge:?}");
    assert!(dbg.contains("[REDACTED]") && !dbg.contains("654321"));
    assert!(!event.raw_redacted.to_string().contains(WEBHOOK_CANARY));
}
