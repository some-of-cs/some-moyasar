//! E2E Rocket integration tests for payments, payment intents, tokens, and card auths.

use rocket::http::{ContentType, Status};
use rocket::local::asynchronous::Client;
use serde_json::{Value, json};
use some_moyasar::prelude::{
    CardAuthStatus, Currency, MoyasarMockServer, PaymentIntentStatus, PaymentStatus, Token,
    TokenStatus,
};

use super::routes_payments_tokens::{
    CardAuthResponse, CreateIntentRequest, PaymentIntentResponse, PaymentResponse,
};
use super::{MockResponseSpec, mount_json_endpoint, spawn_test_client};

/// Mounts a `200 OK` `GET` JSON responder on the mock server.
async fn mount_ok_get(server: &MoyasarMockServer, path: &str, body: Value) {
    let spec = MockResponseSpec { status: 200, body };
    mount_json_endpoint(server, "GET", path, spec).await;
}

/// Mounts a `200 OK` `POST` JSON responder on the mock server.
async fn mount_ok_post(server: &MoyasarMockServer, path: &str, body: Value) {
    let spec = MockResponseSpec { status: 200, body };
    mount_json_endpoint(server, "POST", path, spec).await;
}

/// Dispatches a `POST` request to `path` and deserializes the [`PaymentResponse`] JSON body.
async fn post_payment_action(client: &Client, path: &str) -> PaymentResponse {
    let response = client.post(path).dispatch().await;
    assert_eq!(response.status(), Status::Ok);
    response.into_json().await.expect("valid PaymentResponse")
}

/// Builds a deterministic Moyasar `Payment` JSON fixture for mock responses.
fn sample_payment_json(id: &str, status: &str) -> Value {
    json!({
        "id": id,
        "status": status,
        "amount": 15_000,
        "amount_format": "150.00 SAR",
        "fee": 350,
        "fee_format": "3.50 SAR",
        "refunded": 15_000,
        "refunded_format": "150.00 SAR",
        "captured": 15_000,
        "captured_format": "150.00 SAR",
        "currency": "SAR",
        "source": { "type": "creditcard", "company": "mada" },
        "created_at": "2026-11-15T08:00:00Z",
        "updated_at": "2026-11-15T08:01:00Z"
    })
}

/// Builds a deterministic Moyasar `PaymentIntent` JSON fixture for mock responses.
fn sample_intent_json(id: &str, status: &str) -> Value {
    json!({
        "id": id,
        "status": status,
        "amount": 25_000,
        "amount_format": "250.00 SAR",
        "currency": "SAR",
        "callback_url": "https://merchant.example.com/intent-cb",
        "client_secret": "cs_test_rocket_secret_01",
        "expires_at": "2026-11-15T09:00:00Z",
        "created_at": "2026-11-15T08:00:00Z"
    })
}

/// Builds a deterministic Moyasar `Token` JSON fixture for mock responses.
fn sample_token_json(id: &str) -> Value {
    json!({
        "id": id,
        "status": "active",
        "number": "XXXX-XXXX-XXXX-1111",
        "company": "visa",
        "gateway_id": "gw_rocket_01",
        "created_at": "2026-11-15T08:00:00Z"
    })
}

/// Builds a deterministic Moyasar `CardAuthentication` JSON fixture for mock responses.
fn sample_card_auth_json(id: &str) -> Value {
    json!({
        "id": id,
        "status": "initiated",
        "transaction_url": "https://api.moyasar.com/v1/card_auth/ca_01/prepare",
        "result": null
    })
}

/// Verifies `GET /payments/<id>` and `POST /payments/<id>/void` over the tracked Rocket client.
#[tokio::test]
async fn retrieves_and_voids_payment() {
    let (server, client) = spawn_test_client().await;
    let auth_body = sample_payment_json("pay_01", "authorized");
    let void_body = sample_payment_json("pay_01", "voided");
    let void_api = "/v1/payments/pay_01/void";

    mount_ok_get(&server, "/v1/payments/pay_01", auth_body).await;
    mount_ok_post(&server, void_api, void_body).await;

    let get_resp = client.get("/payments/pay_01").dispatch().await;
    assert_eq!(get_resp.status(), Status::Ok);
    let fetched: PaymentResponse = get_resp.into_json().await.expect("valid payment");
    assert_eq!(fetched.status, PaymentStatus::Authorized);
    assert_eq!(fetched.amount, 15_000);

    let voided = post_payment_action(&client, "/payments/pay_01/void").await;
    assert_eq!(voided.status, PaymentStatus::Voided);

    let bad_resp = client.get("/payments/invalid_id").dispatch().await;
    assert_eq!(bad_resp.status(), Status::BadRequest);
}

/// Verifies `POST /payments/<id>/capture` and `POST /payments/<id>/refund` over Rocket.
#[tokio::test]
async fn captures_and_refunds_payment() {
    let (server, client) = spawn_test_client().await;
    let cap_body = sample_payment_json("pay_02", "captured");
    let ref_body = sample_payment_json("pay_02", "refunded");
    let cap_api = "/v1/payments/pay_02/capture";
    let ref_api = "/v1/payments/pay_02/refund";

    mount_ok_post(&server, cap_api, cap_body).await;
    mount_ok_post(&server, ref_api, ref_body).await;

    let captured = post_payment_action(&client, "/payments/pay_02/capture").await;
    assert_eq!(captured.status, PaymentStatus::Captured);
    assert_eq!(captured.captured, 15_000);

    let refunded = post_payment_action(&client, "/payments/pay_02/refund").await;
    assert_eq!(refunded.status, PaymentStatus::Refunded);
    assert_eq!(refunded.refunded, 15_000);
}

/// Verifies creating, retrieving, and canceling a payment intent over Rocket.
#[tokio::test]
async fn creates_retrieves_and_cancels_payment_intent() {
    let (server, client) = spawn_test_client().await;
    let get_api = "/v1/payment_intents/pi_01";
    let cancel_api = "/v1/payment_intents/pi_01/cancel";
    let created_body = sample_intent_json("pi_01", "created");
    let canceled_body = sample_intent_json("pi_01", "canceled");

    mount_json_endpoint(
        &server,
        "POST",
        "/v1/payment_intents",
        MockResponseSpec {
            status: 201,
            body: sample_intent_json("pi_01", "created"),
        },
    )
    .await;
    mount_ok_get(&server, get_api, created_body).await;
    mount_ok_post(&server, cancel_api, canceled_body).await;

    let req_body = CreateIntentRequest {
        amount: 25_000,
        currency: Currency::SAR,
        callback_url: "https://merchant.example.com/intent-cb".to_owned(),
    };
    let req_json = serde_json::to_string(&req_body).expect("valid json");
    let create_resp = client
        .post("/payment_intents")
        .header(ContentType::JSON)
        .body(req_json)
        .dispatch()
        .await;
    assert_eq!(create_resp.status(), Status::Created);
    let created: PaymentIntentResponse = create_resp.into_json().await.expect("created");
    assert_eq!(created.status, PaymentIntentStatus::Created);

    let get_resp = client.get("/payment_intents/pi_01").dispatch().await;
    assert_eq!(get_resp.status(), Status::Ok);
    let fetched: PaymentIntentResponse = get_resp.into_json().await.expect("fetched");
    assert_eq!(fetched.id.as_str(), "pi_01");

    let cancel_path = "/payment_intents/pi_01/cancel";
    let cancel_resp = client.post(cancel_path).dispatch().await;
    let canceled: PaymentIntentResponse = cancel_resp.into_json().await.expect("canceled");
    assert_eq!(canceled.status, PaymentIntentStatus::Canceled);
}

/// Verifies `GET /tokens/<id>` and `DELETE /tokens/<id>` over the tracked Rocket client.
#[tokio::test]
async fn retrieves_and_deletes_token() {
    let (server, client) = spawn_test_client().await;
    let tok_body = sample_token_json("token_01");

    mount_ok_get(&server, "/v1/tokens/token_01", tok_body).await;
    mount_json_endpoint(
        &server,
        "DELETE",
        "/v1/tokens/token_01",
        MockResponseSpec {
            status: 204,
            body: json!(null),
        },
    )
    .await;

    let get_resp = client.get("/tokens/token_01").dispatch().await;
    assert_eq!(get_resp.status(), Status::Ok);
    let token: Token = get_resp.into_json().await.expect("valid token");
    assert_eq!(token.id.as_str(), "token_01");
    assert_eq!(token.status, TokenStatus::Active);
    assert_eq!(token.last_four(), Some("1111"));

    let del_resp = client.delete("/tokens/token_01").dispatch().await;
    assert_eq!(del_resp.status(), Status::NoContent);
}

/// Verifies `GET /card_auths/<id>` returns `CardAuthResponse` and rejects invalid prefixes.
#[tokio::test]
async fn retrieves_card_authentication_and_rejects_bad_prefix() {
    let (server, client) = spawn_test_client().await;
    let ca_body = sample_card_auth_json("ca_01");

    mount_ok_get(&server, "/v1/card_auths/ca_01", ca_body).await;

    let ok_resp = client.get("/card_auths/ca_01").dispatch().await;
    assert_eq!(ok_resp.status(), Status::Ok);
    let auth: CardAuthResponse = ok_resp.into_json().await.expect("valid card auth");
    assert_eq!(auth.id.as_str(), "ca_01");
    assert_eq!(auth.status, CardAuthStatus::Initiated);
    assert!(auth.transaction_url.is_some());

    let bad_resp = client.get("/card_auths/bad_prefix").dispatch().await;
    assert_eq!(bad_resp.status(), Status::UnprocessableEntity);
}
