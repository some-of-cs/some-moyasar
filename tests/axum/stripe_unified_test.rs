//! E2E tests for `CustomersService`, `SubscriptionsService`, and `UnifiedPaymentGateway` routes.
#![forbid(unsafe_code)]

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use chrono::{Duration, TimeZone, Utc};
use serde_json::{Value, json};
use some_moyasar::prelude::derive_subscription_given_id;
use some_moyasar::test_utils::fixtures::FIXTURE_TIMESTAMP;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::spawn_test_app;

/// Seed parameters for building a mocked Moyasar token payment JSON fixture.
#[derive(Clone, Copy)]
struct PaymentSeed {
    /// Moyasar payment ID (`pay_...`).
    id: &'static str,
    /// Whether the payment has been voided (`true`) or is newly paid (`false`).
    is_voided: bool,
    /// Payment amount in minor units.
    amount: u64,
}

/// Configuration for mounting a saved-token lookup + payment charge mock pair.
#[derive(Clone, Copy)]
struct TokenChargeMock {
    /// Saved card token ID (`token_...`).
    token_id: &'static str,
    /// Resulting payment fixture seed.
    payment: PaymentSeed,
    /// Expected `given_id` idempotency UUID on `POST /v1/payments`.
    given_id: Uuid,
}

/// Builds a Moyasar token payment JSON payload for customer, subscription, and unified tests.
fn token_payment_json(seed: PaymentSeed) -> Value {
    let recent_ts = (Utc::now() - Duration::minutes(10)).to_rfc3339();
    let status = if seed.is_voided { "voided" } else { "paid" };
    let captured = if seed.is_voided { 0_u64 } else { seed.amount };
    let captured_fmt = if seed.is_voided {
        "0.00 SAR"
    } else {
        "150.00 SAR"
    };
    json!({
        "id": seed.id,
        "status": status,
        "amount": seed.amount,
        "amount_format": "150.00 SAR",
        "fee": 0,
        "fee_format": "0.00 SAR",
        "refunded": 0,
        "refunded_format": "0.00 SAR",
        "captured": captured,
        "captured_format": captured_fmt,
        "currency": "SAR",
        "source": {"type": "token"},
        "metadata": {"customer_id": "cus_axum_101", "order_id": "ord_101"},
        "voided_at": seed.is_voided.then_some(recent_ts.as_str()),
        "created_at": recent_ts,
        "updated_at": recent_ts
    })
}

/// Mounts both `GET /v1/tokens/{id}` and `POST /v1/payments` mocks for a token charge flow.
async fn mount_token_charge(server: &MockServer, spec: TokenChargeMock) {
    let token_path = format!("/v1/tokens/{}", spec.token_id);
    let token_body = json!({
        "id": spec.token_id,
        "status": "active",
        "number": "411111XXXXXX1111",
        "company": "visa",
        "gateway_id": "gw_axum_test",
        "created_at": FIXTURE_TIMESTAMP
    });
    Mock::given(method("GET"))
        .and(path(token_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_body))
        .expect(1)
        .mount(server)
        .await;

    let paid = token_payment_json(spec.payment);
    Mock::given(method("POST"))
        .and(path("/v1/payments"))
        .and(body_partial_json(json!({
            "amount": spec.payment.amount,
            "currency": "SAR",
            "given_id": spec.given_id,
            "source": {"type": "token", "token": spec.token_id, "3ds": false}
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(paid))
        .expect(1)
        .mount(server)
        .await;
}

/// Mounts `GET /v1/payments/{id}` and `POST /v1/payments/{id}/void` mocks for same-window voiding.
async fn mount_voidable_payment(server: &MockServer, seed: PaymentSeed) {
    let paid = token_payment_json(PaymentSeed {
        is_voided: false,
        ..seed
    });
    let voided = token_payment_json(PaymentSeed {
        is_voided: true,
        ..seed
    });
    let get_path = format!("/v1/payments/{}", seed.id);
    let void_path = format!("/v1/payments/{}/void", seed.id);
    Mock::given(method("GET"))
        .and(path(get_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(paid))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path(void_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(voided))
        .expect(1)
        .mount(server)
        .await;
}

/// Sends a JSON HTTP request through the Axum router, asserts `expected` status, and parses JSON.
async fn dispatch_expect(
    app: Router,
    http_method: Method,
    uri: &str,
    body: Value,
    expected: StatusCode,
) -> Value {
    let bytes = serde_json::to_vec(&body).expect("serialize request json");
    let req = Request::builder()
        .method(http_method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(bytes))
        .expect("build HTTP request");
    let resp = app.oneshot(req).await.expect("oneshot request succeeds");
    assert_eq!(resp.status(), expected);
    let out_bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read response body");
    serde_json::from_slice(&out_bytes).expect("parse response json")
}

/// Verifies `CustomersService` (`POST /customers/{id}/tokens` + `POST /customers/{id}/charge`).
#[tokio::test]
async fn e2e_customers_attach_token_and_charge() {
    let (mock, router) = spawn_test_app().await;
    let given_id = Uuid::new_v4();
    let attach_body = dispatch_expect(
        router.clone(),
        Method::POST,
        "/customers/cus_axum_101/tokens",
        json!({ "token_id": "token_cus_101" }),
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(attach_body["status"], "attached");

    let upstream = token_payment_json(PaymentSeed {
        id: "pay_cus_101",
        is_voided: false,
        amount: 15000,
    });
    Mock::given(method("POST"))
        .and(path("/v1/payments"))
        .and(body_partial_json(json!({
            "amount": 15000,
            "given_id": given_id,
            "source": {"type": "token", "token": "token_cus_101", "3ds": false},
            "metadata": {"customer_id": "cus_axum_101", "order_id": "ord_101"}
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(upstream))
        .expect(1)
        .mount(mock.server())
        .await;

    let charge_body = dispatch_expect(
        router,
        Method::POST,
        "/customers/cus_axum_101/charge",
        json!({
            "amount": 15000,
            "currency": "SAR",
            "given_id": given_id,
            "metadata": { "order_id": "ord_101" }
        }),
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(charge_body["id"], "pay_cus_101");
    assert_eq!(charge_body["metadata"]["customer_id"], "cus_axum_101");
}

/// Verifies `SubscriptionsService::charge_cycle` (`POST /subscriptions/{id}/charge`).
#[tokio::test]
async fn e2e_subscriptions_charge_cycle() {
    let (mock, router) = spawn_test_app().await;
    let payment = PaymentSeed {
        id: "pay_sub_cycle_2",
        is_voided: false,
        amount: 29900,
    };
    mount_token_charge(
        mock.server(),
        TokenChargeMock {
            token_id: "token_sub_01",
            payment,
            given_id: derive_subscription_given_id("sub_axum_99", 2),
        },
    )
    .await;

    let charge_body = dispatch_expect(
        router,
        Method::POST,
        "/subscriptions/sub_axum_99/charge",
        json!({
            "cycle_index": 2,
            "amount": 29900,
            "currency": "SAR",
            "token_id": "token_sub_01"
        }),
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(charge_body["id"], "pay_sub_cycle_2");
    assert_eq!(charge_body["amount"], 29900);
}

/// Verifies `SubscriptionsService::refund_cycle` (`POST /subscriptions/{id}/refund`).
#[tokio::test]
async fn e2e_subscriptions_refund_cycle() {
    let (mock, router) = spawn_test_app().await;
    mount_voidable_payment(
        mock.server(),
        PaymentSeed {
            id: "pay_sub_cycle_2",
            is_voided: false,
            amount: 29900,
        },
    )
    .await;

    let start = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
    let end = start + Duration::days(30);
    let refund_body = dispatch_expect(
        router,
        Method::POST,
        "/subscriptions/sub_axum_99/refund",
        json!({
            "cycle_payment_id": "pay_sub_cycle_2",
            "period_start": start.to_rfc3339(),
            "period_end": end.to_rfc3339(),
            "strategy": "Full"
        }),
        StatusCode::OK,
    )
    .await;
    assert_eq!(refund_body["payment_id"], "pay_sub_cycle_2");
    assert_eq!(refund_body["action"], "Voided");
    assert_eq!(refund_body["amount_reversed"], 29900);
}

/// Verifies `UnifiedPaymentGateway` (`POST /unified/charge_token` + `POST /unified/reversal`).
#[tokio::test]
async fn e2e_unified_gateway_charge_and_reversal() {
    let (mock, router) = spawn_test_app().await;
    let idem_key = Uuid::new_v4();
    let payment = PaymentSeed {
        id: "pay_uni_01",
        is_voided: false,
        amount: 15000,
    };
    mount_token_charge(
        mock.server(),
        TokenChargeMock {
            token_id: "token_uni_01",
            payment,
            given_id: idem_key,
        },
    )
    .await;
    mount_voidable_payment(mock.server(), payment).await;

    let charge_body = dispatch_expect(
        router.clone(),
        Method::POST,
        "/unified/charge_token",
        json!({
            "amount_minor": 15000,
            "currency": "SAR",
            "token_id": "token_uni_01",
            "idempotency_key": idem_key
        }),
        StatusCode::CREATED,
    )
    .await;
    let rev_body = dispatch_expect(
        router,
        Method::POST,
        "/unified/reversal",
        json!({
            "transaction_id": "pay_uni_01",
            "amount_minor": null
        }),
        StatusCode::OK,
    )
    .await;
    assert_eq!(charge_body["transaction_id"], "pay_uni_01");
    assert_eq!(rev_body["action"], "voided");
    assert_eq!(rev_body["amount_reversed_minor"], 15000);
}
