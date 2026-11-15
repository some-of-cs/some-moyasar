//! E2E gRPC tests for payouts, settlements, transfers, internal transactions, sources, Apple Pay,
//! webhooks API, customers, and subscriptions.

use serde_json::{Value, json};
use some_moyasar::MoyasarMockServer;
use some_moyasar::prelude::{MoyasarClient, SecretKey};
use some_moyasar::test_utils::fixtures::FIXTURE_TIMESTAMP;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::proto_services::{
    ApplePaySessionRpcRequest, BinLookupRpcRequest, CreatePayoutRpcRequest,
    CreateWebhookEndpointRpcRequest, CustomerChargeRpcRequest, ListPageRpcRequest,
    MoyasarPlatformResourcesClient, MoyasarPlatformResourcesServer, RetrieveIdRpcRequest,
};
use super::service_payouts_ledger_sources::MoyasarPlatformResourcesService;

/// Deterministic UUID used across ledger and webhook endpoint E2E RPC tests.
const TEST_ENTITY_UUID: &str = "550e8400-e29b-41d4-a716-446655440000";

/// Wraps `moyasar_client` in a [`MoyasarPlatformResourcesServer`].
fn build_platform_server(
    moyasar_client: MoyasarClient<SecretKey>,
) -> MoyasarPlatformResourcesServer<MoyasarPlatformResourcesService> {
    MoyasarPlatformResourcesServer::new(MoyasarPlatformResourcesService::new(moyasar_client))
}

/// Spawns a loopback Tonic server registering [`MoyasarPlatformResourcesServer`] backed by `mock`.
async fn spawn_platform_grpc_server(
    mock: &MoyasarMockServer,
) -> (
    MoyasarPlatformResourcesClient<tonic::transport::Channel>,
    tokio::sync::oneshot::Sender<()>,
) {
    let secret_key = SecretKey::new("sk_test_tonic_platform").expect("valid test secret key");
    let moyasar_client = MoyasarClient::builder(secret_key)
        .base_url(mock.uri())
        .build()
        .expect("loopback mock URL is valid");
    let server = build_platform_server(moyasar_client);

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

    (MoyasarPlatformResourcesClient::new(channel), shutdown_tx)
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

/// Wraps a single fixture `item` in a paginated list envelope keyed by `key`.
fn ledger_list_fixture_json(key: &str, item: Value) -> Value {
    json!({
        key: [item],
        "meta": {
            "current_page": 1,
            "next_page": null,
            "prev_page": null,
            "total_pages": 1,
            "total_count": 1
        }
    })
}

/// Mounts all payout, settlement, transfer, and internal transaction mocks on `mock.server()`.
async fn mount_payouts_and_ledger_mocks(mock: &MoyasarMockServer) {
    let payout_accounts = ledger_list_fixture_json(
        "payout_accounts",
        json!({
            "id": "pa_mock_01",
            "account_type": "bank",
            "currency": "SAR",
            "properties": {
                "iban": "SA0380000000608010167519",
                "name": "Acme Trading LLC",
                "bank_name": "Al Rajhi Bank"
            },
            "credentials": "secret_upstream_cred",
            "created_at": FIXTURE_TIMESTAMP
        }),
    );
    let payout = json!({
        "id": "pout_mock_01",
        "channel": "sarie",
        "status": "queued",
        "amount": 50000,
        "message": "Disbursement processed",
        "failure_reason": null
    });
    let balance = json!({
        "currency": "SAR",
        "available": 120000,
        "pending": 30000,
        "reserved": 5000,
        "available_format": "1,200.00 SAR",
        "pending_format": "300.00 SAR",
        "reserved_format": "50.00 SAR"
    });
    let settlement = json!({
        "id": "set_mock_01",
        "recipient_type": "entity",
        "currency": "SAR",
        "source_currency": "SAR",
        "amount": 100000,
        "fee": 2000,
        "tax": 300,
        "reference": "SET-REF-01",
        "settlement_count": 4,
        "created_at": FIXTURE_TIMESTAMP
    });
    let transfers = ledger_list_fixture_json(
        "transfers",
        json!({
            "id": "tr_mock_01",
            "recipient_type": "entity",
            "recipient_id": TEST_ENTITY_UUID,
            "amount": 75000,
            "fee": 1500,
            "tax": 225,
            "currency": "SAR",
            "reference": "TR-REF-01",
            "created_at": FIXTURE_TIMESTAMP
        }),
    );
    let internal_txns = ledger_list_fixture_json(
        "internal_transactions",
        json!({
            "id": "itx_mock_01",
            "recipient_type": "entity",
            "recipient_id": TEST_ENTITY_UUID,
            "amount": 45000,
            "currency": "SAR",
            "transfer_id": "tr_mock_01",
            "settled_at": FIXTURE_TIMESTAMP,
            "created_at": FIXTURE_TIMESTAMP,
            "updated_at": FIXTURE_TIMESTAMP
        }),
    );

    mount_json_mock(mock, "GET", "/v1/payout_accounts", payout_accounts).await;
    mount_json_mock(mock, "POST", "/v1/payouts", payout).await;
    mount_json_mock(mock, "GET", "/v1/settlements/balance", balance).await;
    mount_json_mock(mock, "GET", "/v1/settlements/set_mock_01", settlement).await;
    mount_json_mock(mock, "GET", "/v1/transfers", transfers).await;
    mount_json_mock(mock, "GET", "/v1/internal_transactions", internal_txns).await;
}

/// Mounts BIN issuer, Apple Pay session, and webhook endpoint mocks on `mock.server()`.
async fn mount_sources_and_webhooks_mocks(mock: &MoyasarMockServer) {
    let issuer = json!({
        "issuer_name": "AL RAJHI BANK",
        "issuer_country": "SA",
        "issuer_card_type": "debit",
        "issuer_card_category": "classic",
        "first_digits": "420132",
        "last_digits": null
    });
    let apple_pay = json!({
        "epochTimestamp": 1700000000000_u64,
        "expiresAt": 1700003600000_u64,
        "merchantSessionIdentifier": "SSH_TONIC_01",
        "nonce": "abcdef12",
        "merchantIdentifier": "merchant.com.some_cs",
        "domainName": "checkout.example.com",
        "displayName": "Some Store",
        "signature": "MIAGCSqGSIb3DQEHAqCAMIACAQEx"
    });
    let webhook = json!({
        "id": TEST_ENTITY_UUID,
        "http_method": "post",
        "url": "https://merchant.example/webhooks",
        "shared_secret": "whsec_tonic_secret_never_leak",
        "events": ["payment_paid", "payment_failed"],
        "created_at": FIXTURE_TIMESTAMP
    });
    mount_json_mock(mock, "POST", "/v1/source/issuer", issuer).await;
    mount_json_mock(mock, "POST", "/v1/applepay/initiate", apple_pay).await;
    mount_json_mock(mock, "POST", "/v1/webhooks", webhook).await;
}

#[tokio::test]
async fn payouts_and_settlements_ledger_rpc_e2e() {
    let mock = MoyasarMockServer::start().await;
    mount_payouts_and_ledger_mocks(&mock).await;
    let (mut client, shutdown_tx) = spawn_platform_grpc_server(&mock).await;

    let accounts = client
        .list_payout_accounts(ListPageRpcRequest { page: Some(1) })
        .await
        .expect("list_payout_accounts RPC should succeed")
        .into_inner();
    assert_eq!(accounts.id, "count:1");
    assert!(accounts.json.contains("pa_mock_01"));
    assert!(!accounts.json.contains("secret_upstream_cred"));

    let payout = client
        .create_payout(CreatePayoutRpcRequest {
            source_id: "pa_mock_01".to_owned(),
            amount_minor: 50_000,
            iban: "SA0380000000608010167519".to_owned(),
            name: "Vendor Co".to_owned(),
            mobile: "0515555555".to_owned(),
        })
        .await
        .expect("create_payout RPC should succeed")
        .into_inner();
    assert_eq!(payout.id, "pout_mock_01");
    assert!(payout.json.contains("queued"));

    let balance = client
        .fetch_settlement_balance(ListPageRpcRequest { page: None })
        .await
        .expect("fetch_settlement_balance RPC should succeed")
        .into_inner();
    assert_eq!(balance.id, "SAR");
    assert!(balance.json.contains("120000"));

    let settlement = client
        .retrieve_settlement(RetrieveIdRpcRequest {
            id: "set_mock_01".to_owned(),
        })
        .await
        .expect("retrieve_settlement RPC should succeed")
        .into_inner();
    assert_eq!(settlement.id, "set_mock_01");
    assert!(settlement.json.contains("SET-REF-01"));

    let transfers = client
        .list_transfers(ListPageRpcRequest { page: Some(1) })
        .await
        .expect("list_transfers RPC should succeed")
        .into_inner();
    assert_eq!(transfers.id, "count:1");
    assert!(transfers.json.contains("tr_mock_01"));

    let internal_txns = client
        .list_internal_transactions(ListPageRpcRequest { page: None })
        .await
        .expect("list_internal_transactions RPC should succeed")
        .into_inner();
    assert_eq!(internal_txns.id, "count:1");
    assert!(internal_txns.json.contains("itx_mock_01"));

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn sources_apple_pay_and_webhooks_api_rpc_e2e() {
    let mock = MoyasarMockServer::start().await;
    mount_sources_and_webhooks_mocks(&mock).await;
    let (mut client, shutdown_tx) = spawn_platform_grpc_server(&mock).await;

    let issuer = client
        .lookup_issuer(BinLookupRpcRequest {
            first_digits: "420132".to_owned(),
        })
        .await
        .expect("lookup_issuer RPC should succeed")
        .into_inner();
    assert_eq!(issuer.id, "420132");
    assert_eq!(issuer.json, "Debit");

    let validation_url = "https://apple-pay-gateway.apple.com/paymentservices/paymentSession";
    let session = client
        .initiate_apple_pay_session(ApplePaySessionRpcRequest {
            validation_url: validation_url.to_owned(),
            display_name: "Some Store".to_owned(),
            domain_name: "checkout.example.com".to_owned(),
        })
        .await
        .expect("initiate_apple_pay_session RPC should succeed")
        .into_inner();
    assert_eq!(session.id, "merchant.com.some_cs");
    assert!(session.json.contains("SSH_TONIC_01"));

    let webhook = client
        .manage_webhook_endpoint(CreateWebhookEndpointRpcRequest {
            url: "https://merchant.example/webhooks".to_owned(),
            shared_secret: "whsec_tonic_secret_never_leak".to_owned(),
        })
        .await
        .expect("manage_webhook_endpoint RPC should succeed")
        .into_inner();
    assert_eq!(webhook.id, TEST_ENTITY_UUID);
    assert_eq!(webhook.json, "https://merchant.example/webhooks");
    assert!(!webhook.id.contains("whsec_tonic_secret_never_leak"));
    assert!(!webhook.json.contains("whsec_tonic_secret_never_leak"));
    assert!(!webhook.json.contains("shared_secret"));

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn customers_and_subscriptions_rpc_e2e() {
    let mock = MoyasarMockServer::start().await;
    mock.mount_default_routes().await;
    let (mut client, shutdown_tx) = spawn_platform_grpc_server(&mock).await;

    let res = client
        .charge_customer_and_subscription(CustomerChargeRpcRequest {
            customer_id: "cus_tonic_01".to_owned(),
            token_id: "token_mock_01".to_owned(),
            amount_minor: 15_000,
            currency: "SAR".to_owned(),
            given_id: Uuid::new_v4().to_string(),
        })
        .await
        .expect("charge_customer_and_subscription RPC should succeed")
        .into_inner();
    assert_eq!(res.id, "pay_mock_01");
    assert_eq!(res.json, "pay_mock_01");

    let _ = shutdown_tx.send(());
}
