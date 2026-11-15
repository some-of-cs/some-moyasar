//! Tonic gRPC server bootstrap example for `some-moyasar`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::future::Future;
use std::net::{AddrParseError, SocketAddr};

use some_moyasar::kernel::auth::SecretKey;
use some_moyasar::kernel::client::MoyasarClient;

pub mod proto;
pub mod service;
pub mod status_mapper;

use proto::MoyasarPaymentGatewayServer;
use service::MoyasarGrpcService;

/// Default gRPC listen address used when `TONIC_LISTEN_ADDR` is unset or empty.
pub const DEFAULT_LISTEN_ADDR: &str = "[::1]:50051";

/// Builds a [`MoyasarClient<SecretKey>`] from `$MOYASAR_SECRET_KEY` and
/// `$MOYASAR_WEBHOOK_SECRET` environment variable placeholders.
///
/// # Errors
///
/// Returns an error if either secret key or webhook secret fails validation or
/// cannot be resolved from the environment.
pub fn build_moyasar_client() -> Result<MoyasarClient<SecretKey>, Box<dyn std::error::Error>> {
    let client: MoyasarClient<SecretKey> = MoyasarClient::builder("$MOYASAR_SECRET_KEY")
        .webhook_secret("$MOYASAR_WEBHOOK_SECRET")
        .build()?;
    Ok(client)
}

/// Parses a gRPC listen [`SocketAddr`] from an optional raw string, falling back
/// to [`DEFAULT_LISTEN_ADDR`] when `None` or whitespace-only.
///
/// # Errors
///
/// Returns an [`AddrParseError`] if the resolved address string is invalid.
pub fn parse_listen_addr_from(raw: Option<&str>) -> Result<SocketAddr, AddrParseError> {
    let candidate = raw
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_LISTEN_ADDR);
    candidate.parse()
}

/// Resolves the gRPC listen [`SocketAddr`] from `TONIC_LISTEN_ADDR` or falls
/// back to [`DEFAULT_LISTEN_ADDR`].
///
/// # Errors
///
/// Returns an [`AddrParseError`] if the configured address is not a valid socket address.
pub fn parse_listen_addr() -> Result<SocketAddr, AddrParseError> {
    let env_value = std::env::var("TONIC_LISTEN_ADDR").ok();
    parse_listen_addr_from(env_value.as_deref())
}

/// Returns a shutdown future that stays pending until external termination.
///
/// `tokio` in `Cargo.toml` does not enable the `signal` feature, so this helper
/// uses `std::future::pending` for `serve_with_shutdown` without requiring `tokio::signal`.
pub fn pending_shutdown() -> impl Future<Output = ()> {
    std::future::pending::<()>()
}

/// Entry point for the `some-moyasar` Tonic gRPC payment gateway server.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = build_moyasar_client()?;
    let addr = parse_listen_addr()?;
    let svc = MoyasarPaymentGatewayServer::new(MoyasarGrpcService::new(client));

    tonic::transport::Server::builder()
        .add_service(svc)
        .serve_with_shutdown(addr, pending_shutdown())
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_listen_addr_from_uses_default_when_none_or_blank() {
        let expected: SocketAddr = DEFAULT_LISTEN_ADDR.parse().expect("valid default addr");
        assert_eq!(parse_listen_addr_from(None), Ok(expected));
        assert_eq!(parse_listen_addr_from(Some("")), Ok(expected));
        assert_eq!(parse_listen_addr_from(Some("   ")), Ok(expected));
    }

    #[test]
    fn parse_listen_addr_from_accepts_valid_custom_addr() {
        let expected: SocketAddr = "127.0.0.1:50052".parse().expect("valid custom addr");
        assert_eq!(
            parse_listen_addr_from(Some(" 127.0.0.1:50052 ")),
            Ok(expected)
        );
    }

    #[test]
    fn parse_listen_addr_from_rejects_invalid_addr() {
        assert!(parse_listen_addr_from(Some("not-a-socket-addr")).is_err());
    }
}
