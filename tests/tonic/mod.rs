//! Loopback Tonic gRPC test harness.

#[path = "../../examples/tonic/proto.rs"]
pub mod proto;
#[path = "../../examples/tonic/service.rs"]
pub mod service;
#[path = "../../examples/tonic/status_mapper.rs"]
pub mod status_mapper;

mod unified_rpc_test;
mod webhook_rpc_test;

use some_moyasar::MoyasarMockServer;
use some_moyasar::prelude::{MoyasarClient, SecretKey};

/// Spawns an in-process Tonic gRPC server bound to `127.0.0.1:0` backed by `mock`
/// and returns a connected [`proto::MoyasarPaymentGatewayClient`] alongside a
/// graceful shutdown signal sender.
pub async fn spawn_test_grpc_server(
    mock: &MoyasarMockServer,
    webhook_secret: &str,
) -> (
    proto::MoyasarPaymentGatewayClient<tonic::transport::Channel>,
    tokio::sync::oneshot::Sender<()>,
) {
    let secret_key = SecretKey::new("sk_test_tonic_e2e").expect("valid test secret key");
    let moyasar_client = MoyasarClient::builder(secret_key)
        .base_url(mock.uri())
        .webhook_secret(webhook_secret)
        .build()
        .expect("loopback mock URL is valid");

    let grpc_service = service::MoyasarGrpcService::new(moyasar_client);
    let server = proto::MoyasarPaymentGatewayServer::new(grpc_service);

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

    (proto::MoyasarPaymentGatewayClient::new(channel), shutdown_tx)
}
