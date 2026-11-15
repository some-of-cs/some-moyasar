//! E2E tests for unified payment gRPC RPCs.

use some_moyasar::MoyasarMockServer;
use uuid::Uuid;

use super::proto::{
    ChargeTokenRpcRequest, CreateIntentRpcRequest, RefundOrVoidRpcRequest,
};
use super::spawn_test_grpc_server;

#[tokio::test]
async fn create_intent_rpc_succeeds() {
    let mock = MoyasarMockServer::start().await;
    mock.mount_default_routes().await;
    let (mut client, shutdown_tx) = spawn_test_grpc_server(&mock, "whsec_test_tonic").await;

    let response = client
        .create_intent(CreateIntentRpcRequest {
            amount_minor: 15_000,
            currency: "SAR".to_owned(),
            description: Some("Tonic E2E intent".to_owned()),
            callback_url: "https://merchant.example/cb".to_owned(),
        })
        .await
        .expect("CreateIntent RPC should succeed")
        .into_inner();

    assert_eq!(response.id, "pi_mock_01");
    assert_eq!(response.amount_minor, 15_000);
    assert_eq!(response.currency, "SAR");
    assert_eq!(response.client_secret.as_deref(), Some("pi_secret_mock"));

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn charge_saved_token_rpc_with_given_id_idempotency() {
    let mock = MoyasarMockServer::start().await;
    mock.mount_default_routes().await;
    let (mut client, shutdown_tx) = spawn_test_grpc_server(&mock, "whsec_test_tonic").await;

    let idempotency_key = Uuid::new_v4().to_string();
    let response = client
        .charge_saved_token(ChargeTokenRpcRequest {
            amount_minor: 15_000,
            currency: "SAR".to_owned(),
            token_id: "token_mock_01".to_owned(),
            idempotency_key,
        })
        .await
        .expect("ChargeSavedToken RPC should succeed")
        .into_inner();

    assert_eq!(response.transaction_id, "pay_mock_01");
    assert_eq!(response.status, "paid");
    assert_eq!(response.amount_minor, 15_000);
    assert_eq!(response.currency, "SAR");

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn refund_or_void_rpc_smart_routing() {
    let mock = MoyasarMockServer::start().await;
    mock.mount_default_routes().await;
    let (mut client, shutdown_tx) = spawn_test_grpc_server(&mock, "whsec_test_tonic").await;

    let void_res = client
        .refund_or_void(RefundOrVoidRpcRequest {
            transaction_id: "pay_mock_01".to_owned(),
            amount_minor: None,
        })
        .await
        .expect("RefundOrVoid (void) RPC should succeed")
        .into_inner();

    assert_eq!(void_res.transaction_id, "pay_mock_01");
    assert_eq!(void_res.action, "voided");

    let refund_res = client
        .refund_or_void(RefundOrVoidRpcRequest {
            transaction_id: "pay_mock_01".to_owned(),
            amount_minor: Some(5_000),
        })
        .await
        .expect("RefundOrVoid (partial refund) RPC should succeed")
        .into_inner();

    assert_eq!(refund_res.transaction_id, "pay_mock_01");
    assert_eq!(refund_res.action, "refunded");

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn negative_amount_returns_invalid_argument() {
    let mock = MoyasarMockServer::start().await;
    mock.mount_default_routes().await;
    let (mut client, shutdown_tx) = spawn_test_grpc_server(&mock, "whsec_test_tonic").await;

    let status = client
        .create_intent(CreateIntentRpcRequest {
            amount_minor: -100,
            currency: "SAR".to_owned(),
            description: None,
            callback_url: "https://merchant.example/cb".to_owned(),
        })
        .await
        .expect_err("negative amount_minor must fail with InvalidArgument");

    assert_eq!(status.code(), tonic::Code::InvalidArgument);

    let _ = shutdown_tx.send(());
}
