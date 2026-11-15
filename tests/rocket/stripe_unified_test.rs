//! E2E Rocket integration tests for Stripe compatibility services and unified payment gateway.

use rocket::http::{ContentType, Status};
use rocket::local::asynchronous::{Client, LocalResponse};
use serde::Serialize;
use serde_json::{Value, json};
use some_moyasar::prelude::{
    Currency, MoyasarMockServer, PaymentStatus, RefundCreateParams, RefundReceipt,
    UnifiedChargeResponse, UnifiedIntentResponse, UnifiedReversalResponse,
};
use uuid::Uuid;

use super::routes_stripe_unified::*;
use super::{MockResponseSpec, mount_json_endpoint, spawn_test_client};

/// Builds a deterministic Moyasar payment JSON payload for mock responses.
fn sample_payment_json(id: &str, status: &str, amount: u64, refunded: u64) -> Value {
    let refunded_fmt = if refunded > 0 { "50.00 SAR" } else { "0.00 SAR" };
    json!({
        "id": id,
        "status": status,
        "amount": amount,
        "amount_format": "150.00 SAR",
        "fee": 0,
        "fee_format": "0.00 SAR",
        "refunded": refunded,
        "refunded_format": refunded_fmt,
        "captured": amount,
        "captured_format": "150.00 SAR",
        "currency": "SAR",
        "source": { "type": "token" },
        "refunded_at": (status == "refunded").then_some("2026-11-15T08:05:00Z"),
        "voided_at": null,
        "created_at": "2026-11-15T08:00:00Z",
        "updated_at": "2026-11-15T08:05:00Z"
    })
}

/// Builds a deterministic active Moyasar token JSON payload for mock responses.
fn sample_token_json(id: &str) -> Value {
    json!({
        "id": id,
        "status": "active",
        "number": "411111XXXXXX1111",
        "company": "visa",
        "gateway_id": "gw_rocket_1",
        "created_at": "2026-11-15T08:00:00Z"
    })
}

/// Builds a deterministic Moyasar payment intent JSON payload for mock responses.
fn sample_intent_json(id: &str, amount: u64) -> Value {
    json!({
        "id": id,
        "status": "created",
        "amount": amount,
        "amount_format": "150.00 SAR",
        "currency": "SAR",
        "client_secret": "pi_secret_rocket_1",
        "expires_at": "2026-11-15T09:00:00Z",
        "created_at": "2026-11-15T08:00:00Z",
        "updated_at": "2026-11-15T08:00:00Z"
    })
}

/// Dispatches a JSON `POST` request to `uri` through the tracked Rocket client.
async fn post_json<'a, T: Serialize>(
    client: &'a Client,
    uri: &'a str,
    body: &T,
) -> LocalResponse<'a> {
    let payload = serde_json::to_string(body).expect("request DTO should serialize");
    let req = client.post(uri).header(ContentType::JSON).body(payload);
    req.dispatch().await
}

/// Mounts mock endpoints for unified intent creation, token charge, and partial refund.
async fn mount_unified_gateway_mocks(server: &MoyasarMockServer) {
    mount_json_endpoint(
        server,
        "POST",
        "/v1/payment_intents",
        MockResponseSpec {
            status: 201,
            body: sample_intent_json("pi_uni_1", 15000),
        },
    )
    .await;
    mount_json_endpoint(
        server,
        "GET",
        "/v1/tokens/token_uni_1",
        MockResponseSpec {
            status: 200,
            body: sample_token_json("token_uni_1"),
        },
    )
    .await;
    mount_json_endpoint(
        server,
        "POST",
        "/v1/payments",
        MockResponseSpec {
            status: 201,
            body: sample_payment_json("pay_uni_1", "paid", 15000, 0),
        },
    )
    .await;
    mount_json_endpoint(
        server,
        "GET",
        "/v1/payments/pay_uni_1",
        MockResponseSpec {
            status: 200,
            body: sample_payment_json("pay_uni_1", "paid", 15000, 0),
        },
    )
    .await;
    mount_json_endpoint(
        server,
        "POST",
        "/v1/payments/pay_uni_1/refund",
        MockResponseSpec {
            status: 200,
            body: sample_payment_json("pay_uni_1", "refunded", 15000, 5000),
        },
    )
    .await;
}

/// Verifies `POST /stripe/refunds` synthesizes a `RefundReceipt` via `RefundsService`.
#[tokio::test]
async fn creates_stripe_compat_refund_receipt() {
    let (server, client) = spawn_test_client().await;
    mount_json_endpoint(
        &server,
        "GET",
        "/v1/payments/pay_ref_1",
        MockResponseSpec {
            status: 200,
            body: sample_payment_json("pay_ref_1", "paid", 15000, 0),
        },
    )
    .await;
    mount_json_endpoint(
        &server,
        "POST",
        "/v1/payments/pay_ref_1/refund",
        MockResponseSpec {
            status: 200,
            body: sample_payment_json("pay_ref_1", "refunded", 15000, 5000),
        },
    )
    .await;

    let req = RefundCreateParams::new("pay_ref_1")
        .amount(5000)
        .prefer_void_if_unsettled(false);
    let response = post_json(&client, "/stripe/refunds", &req).await;

    assert_eq!(response.status(), Status::Ok);
    let receipt: RefundReceipt = response.into_json().await.expect("refund receipt");
    assert_eq!(receipt.payment_id, "pay_ref_1");
    assert_eq!(receipt.status, PaymentStatus::Refunded);
    assert_eq!(receipt.refunded_amount, 5000);
    assert_eq!(receipt.currency, Currency::SAR);
}

/// Verifies attaching a token to the customer vault and charging the customer off-session.
#[tokio::test]
async fn attaches_customer_token_and_charges_customer() {
    let (server, client) = spawn_test_client().await;
    mount_json_endpoint(
        &server,
        "GET",
        "/v1/tokens/token_cus_1",
        MockResponseSpec {
            status: 200,
            body: sample_token_json("token_cus_1"),
        },
    )
    .await;
    mount_json_endpoint(
        &server,
        "POST",
        "/v1/payments",
        MockResponseSpec {
            status: 201,
            body: sample_payment_json("pay_cus_1", "paid", 15000, 0),
        },
    )
    .await;

    let attach_req = AttachCustomerTokenRequest {
        token_id: "token_cus_1".to_owned(),
    };
    let attach_res = post_json(&client, "/stripe/customers/cus_1/tokens", &attach_req).await;
    assert_eq!(attach_res.status(), Status::NoContent);

    let charge_req = CustomerChargeRequest {
        amount: 15000,
        currency: Currency::SAR,
    };
    let charge_res = post_json(&client, "/stripe/customers/cus_1/charges", &charge_req).await;
    assert_eq!(charge_res.status(), Status::Ok);

    let paid: StripePaymentResponse = charge_res.into_json().await.expect("customer charge");
    assert_eq!(paid.id, "pay_cus_1");
    assert_eq!(paid.status, PaymentStatus::Paid);
    assert_eq!(paid.amount, 15000);
}

/// Verifies `POST /stripe/subscriptions/charge_cycle` validates the token and charges the cycle.
#[tokio::test]
async fn charges_subscription_cycle_with_active_token() {
    let (server, client) = spawn_test_client().await;
    mount_json_endpoint(
        &server,
        "GET",
        "/v1/tokens/token_sub_1",
        MockResponseSpec {
            status: 200,
            body: sample_token_json("token_sub_1"),
        },
    )
    .await;
    mount_json_endpoint(
        &server,
        "POST",
        "/v1/payments",
        MockResponseSpec {
            status: 201,
            body: sample_payment_json("pay_sub_1", "paid", 29900, 0),
        },
    )
    .await;

    let req = SubscriptionChargeRequest {
        subscription_id: "sub_rocket_1".to_owned(),
        cycle_index: 1,
        amount: 29900,
        token_id: "token_sub_1".to_owned(),
    };
    let response = post_json(&client, "/stripe/subscriptions/charge_cycle", &req).await;

    assert_eq!(response.status(), Status::Ok);
    let paid: StripePaymentResponse = response.into_json().await.expect("subscription charge");
    assert_eq!(paid.id, "pay_sub_1");
    assert_eq!(paid.status, PaymentStatus::Paid);
    assert_eq!(paid.amount, 29900);
    assert_eq!(paid.currency, Currency::SAR);
}

/// Verifies unified gateway intent creation, saved-token charge, and partial reversal routes.
#[tokio::test]
async fn executes_unified_gateway_intent_charge_and_reversal() {
    let (server, client) = spawn_test_client().await;
    mount_unified_gateway_mocks(&server).await;

    let intent_req = UnifiedIntentDto {
        amount_minor: 15000,
        currency: "SAR".to_owned(),
        callback_url: "https://merchant.example.com/unified-cb".to_owned(),
    };
    let intent_res = post_json(&client, "/unified/intents", &intent_req).await;
    assert_eq!(intent_res.status(), Status::Created);
    let intent: UnifiedIntentResponse = intent_res.into_json().await.expect("unified intent");
    assert_eq!(intent.id, "pi_uni_1");

    let charge_req = UnifiedChargeDto {
        amount_minor: 15000,
        currency: "SAR".to_owned(),
        token_id: "token_uni_1".to_owned(),
        idempotency_key: Uuid::from_u128(0x1111_2222_3333_4444_5555_6666_7777_8888),
    };
    let charge_res = post_json(&client, "/unified/charges", &charge_req).await;
    let charge: UnifiedChargeResponse = charge_res.into_json().await.expect("unified charge");
    assert_eq!(charge.transaction_id, "pay_uni_1");

    let rev_req = UnifiedReversalDto {
        transaction_id: "pay_uni_1".to_owned(),
        amount_minor: Some(5000),
    };
    let rev_res = post_json(&client, "/unified/reversals", &rev_req).await;
    let reversal: UnifiedReversalResponse = rev_res.into_json().await.expect("reversal");
    assert_eq!(reversal.action, "refunded");
    assert_eq!(reversal.amount_reversed_minor, 5000);
}
