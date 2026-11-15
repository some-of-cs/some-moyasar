//! E2E gRPC tests for payments, payment intents, tokens, invoices, and 3DS card auths.

use chrono::{Duration, Utc};
use serde_json::{Value, json};
use some_moyasar::MoyasarMockServer;
use some_moyasar::prelude::{MoyasarClient, SecretKey};
use some_moyasar::test_utils::fixtures::FIXTURE_TIMESTAMP;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::proto_services::{
    CreateInvoiceRpcRequest, MoyasarCoreResourcesClient, MoyasarCoreResourcesServer,
    RetrieveIdRpcRequest, SmartRefundRpcRequest,
};
use super::service_payments_tokens::MoyasarCoreResourcesService;

/// Deterministic UUID used across invoice E2E RPC tests.
const TEST_INVOICE_ID: &str = "550e8400-e29b-41d4-a716-446655440000";

/// Builds a [`RetrieveIdRpcRequest`] for `resource_id`.
fn id_req(resource_id: &str) -> RetrieveIdRpcRequest {
    RetrieveIdRpcRequest {
        id: resource_id.to_owned(),
    }
}

/// Wraps `moyasar_client` in a [`MoyasarCoreResourcesServer`].
fn build_core_server(
    moyasar_client: MoyasarClient<SecretKey>,
) -> MoyasarCoreResourcesServer<MoyasarCoreResourcesService> {
    MoyasarCoreResourcesServer::new(MoyasarCoreResourcesService::new(moyasar_client))
}

/// Spawns a loopback Tonic server registering [`MoyasarCoreResourcesServer`] backed by `mock`.
async fn spawn_core_grpc_server(
    mock: &MoyasarMockServer,
) -> (
    MoyasarCoreResourcesClient<tonic::transport::Channel>,
    tokio::sync::oneshot::Sender<()>,
) {
    let secret_key = SecretKey::new("sk_test_tonic_core").expect("valid test secret key");
    let moyasar_client = MoyasarClient::builder(secret_key)
        .base_url(mock.uri())
        .build()
        .expect("loopback mock URL is valid");
    let server = build_core_server(moyasar_client);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback listener");
    let addr = listener.local_addr().expect("listener local_addr");
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        let _ = tonic::transport::Server::builder()
            .add_service(server)
            .serve_with_incoming_shutdown(incoming, async {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    let endpoint = tonic::transport::Channel::from_shared(format!("http://{addr}"))
        .expect("valid loopback endpoint URI");
    let channel = endpoint
        .connect()
        .await
        .expect("connect to loopback gRPC server");

    (MoyasarCoreResourcesClient::new(channel), shutdown_tx)
}

/// Mounts a JSON responder on `mock.server()` for `http_method` and `upstream_path`.
async fn mount_json_mock(
    mock: &MoyasarMockServer,
    http_method: &str,
    upstream_path: &str,
    body: Value,
) {
    let is_post = http_method == "POST";
    let code = if is_post { 201 } else { 200 };
    Mock::given(method(http_method))
        .and(path(upstream_path))
        .respond_with(ResponseTemplate::new(code).set_body_json(body))
        .mount(mock.server())
        .await;
}

/// Builds a deterministic `PaymentIntent` JSON fixture with a future `expires_at`.
fn intent_json(status: &str) -> Value {
    let now = Utc::now();
    let expires_at = (now + Duration::minutes(45)).to_rfc3339();
    let created_at = now.to_rfc3339();
    json!({
        "id": "pi_mock_01",
        "status": status,
        "amount": 25000,
        "amount_format": "250.00 SAR",
        "currency": "SAR",
        "description": "Order #42",
        "callback_url": "https://example.com/callback",
        "client_secret": "cs_test_e2e_secret",
        "expires_at": expires_at,
        "created_at": created_at,
        "updated_at": created_at
    })
}

/// Builds a deterministic `Invoice` JSON fixture for `status`.
fn invoice_json(status: &str) -> Value {
    json!({
        "id": TEST_INVOICE_ID,
        "status": status,
        "amount": 25000,
        "currency": "SAR",
        "amount_format": "250.00 SAR",
        "description": "Enterprise subscription invoice",
        "url": "https://moyasar.com/invoices/550e8400-e29b-41d4-a716-446655440000",
        "callback_url": "https://merchant.example/invoices/callback",
        "payments": [],
        "created_at": FIXTURE_TIMESTAMP,
        "updated_at": FIXTURE_TIMESTAMP
    })
}

/// Builds a deterministic `CardAuthentication` JSON fixture including a sensitive CAVV field.
fn card_auth_json() -> Value {
    json!({
        "id": "ca_mock_01",
        "status": "authenticated",
        "transaction_url": "https://acs.moyasar.example/challenge/ca_mock_01",
        "result": {
            "eci": "05",
            "authentication_value": "cavv_test_value",
            "ds_transaction_id": "ds_tx_01",
            "version": "2.2.0",
            "transaction_status": "Y",
            "auth_scheme": "visa",
            "is_frictionless": true
        }
    })
}

/// Mounts token delete, invoice create/cancel, and 3DS card auth mocks on `mock.server()`.
async fn mount_tokens_invoices_card_auth_mocks(mock: &MoyasarMockServer) {
    mock.mount_default_routes().await;
    Mock::given(method("DELETE"))
        .and(path("/v1/tokens/token_mock_01"))
        .respond_with(ResponseTemplate::new(204))
        .mount(mock.server())
        .await;
    mount_json_mock(mock, "POST", "/v1/invoices", invoice_json("created")).await;
    let cancel_path = format!("/v1/invoices/{TEST_INVOICE_ID}/cancel");
    mount_json_mock(mock, "PUT", &cancel_path, invoice_json("canceled")).await;
    mount_json_mock(
        mock,
        "GET",
        "/v1/card_authentications/ca_mock_01",
        card_auth_json(),
    )
    .await;
    mount_json_mock(mock, "GET", "/v1/card_auths/ca_mock_01", card_auth_json()).await;
}

#[tokio::test]
async fn payments_and_smart_refund_rpc_e2e() {
    let mock = MoyasarMockServer::start().await;
    mock.mount_default_routes().await;
    let (mut client, shutdown_tx) = spawn_core_grpc_server(&mock).await;

    let payment = client
        .retrieve_payment(id_req("pay_mock_01"))
        .await
        .expect("retrieve_payment RPC should succeed")
        .into_inner();
    assert_eq!(payment.id, "pay_mock_01");
    assert_eq!(payment.status, "paid");
    assert_eq!(payment.amount_minor, 15_000);
    assert_eq!(payment.currency, "SAR");

    let refund = client
        .smart_refund(SmartRefundRpcRequest {
            payment_id: "pay_mock_01".to_owned(),
            amount_minor: Some(15_000),
        })
        .await
        .expect("smart_refund RPC should succeed")
        .into_inner();
    assert_eq!(refund.payment_id, "pay_mock_01");
    assert_eq!(refund.action, "refunded");
    assert_eq!(refund.amount_minor, 15_000);

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn intents_retrieve_and_cancel_rpc_e2e() {
    let mock = MoyasarMockServer::start().await;
    mount_json_mock(
        &mock,
        "GET",
        "/v1/payment_intents/pi_mock_01",
        intent_json("created"),
    )
    .await;
    mount_json_mock(
        &mock,
        "POST",
        "/v1/payment_intents/pi_mock_01/cancel",
        intent_json("canceled"),
    )
    .await;
    let (mut client, shutdown_tx) = spawn_core_grpc_server(&mock).await;

    let fetched = client
        .retrieve_intent(id_req("pi_mock_01"))
        .await
        .expect("retrieve_intent RPC should succeed")
        .into_inner();
    assert_eq!(fetched.id, "pi_mock_01");
    assert_eq!(fetched.status, "created");
    assert_eq!(fetched.amount_minor, 25_000);
    assert!(fetched.usable);

    let canceled = client
        .cancel_intent(id_req("pi_mock_01"))
        .await
        .expect("cancel_intent RPC should succeed")
        .into_inner();
    assert_eq!(canceled.id, "pi_mock_01");
    assert_eq!(canceled.status, "canceled");
    assert!(!canceled.usable);

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn tokens_invoices_and_card_auths_rpc_e2e() {
    let mock = MoyasarMockServer::start().await;
    mount_tokens_invoices_card_auth_mocks(&mock).await;
    let (mut client, shutdown_tx) = spawn_core_grpc_server(&mock).await;

    let token = client
        .retrieve_token(id_req("token_mock_01"))
        .await
        .expect("retrieve_token RPC should succeed")
        .into_inner();
    assert_eq!(token.id, "token_mock_01");
    assert_eq!(token.status, "active");
    assert_eq!(token.brand, "visa");
    assert!(!token.deleted);

    let deleted = client
        .delete_token(id_req("token_mock_01"))
        .await
        .expect("delete_token RPC should succeed")
        .into_inner();
    assert_eq!(deleted.id, "token_mock_01");
    assert_eq!(deleted.status, "deleted");
    assert!(deleted.deleted);

    let created_inv = client
        .create_invoice(CreateInvoiceRpcRequest {
            amount_minor: 25_000,
            currency: "SAR".to_owned(),
            description: Some("Enterprise subscription invoice".to_owned()),
        })
        .await
        .expect("create_invoice RPC should succeed")
        .into_inner();
    assert_eq!(created_inv.id, TEST_INVOICE_ID);
    assert_eq!(created_inv.status, "created");
    assert_eq!(created_inv.amount_minor, 25_000);

    let canceled_inv = client
        .cancel_invoice(id_req(TEST_INVOICE_ID))
        .await
        .expect("cancel_invoice RPC should succeed")
        .into_inner();
    assert_eq!(canceled_inv.id, TEST_INVOICE_ID);
    assert_eq!(canceled_inv.status, "canceled");

    let card_auth = client
        .retrieve_card_auth(id_req("ca_mock_01"))
        .await
        .expect("retrieve_card_auth RPC should succeed")
        .into_inner();
    assert_eq!(card_auth.id, "ca_mock_01");
    assert_eq!(card_auth.status, "authenticated");
    assert_eq!(
        card_auth.transaction_url.as_deref(),
        Some("https://acs.moyasar.example/challenge/ca_mock_01")
    );

    let _ = shutdown_tx.send(());
}
