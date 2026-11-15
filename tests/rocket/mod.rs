//! Shared Rocket async tracked client harness and E2E test modules.
#![warn(missing_docs)]

use std::time::Duration;

use rocket::local::asynchronous::Client;
use rocket::{Build, Rocket};
use serde_json::Value;
use some_moyasar::prelude::*;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

/// Custom Rocket responder module from `examples/rocket/responder.rs`.
#[path = "../../examples/rocket/responder.rs"]
pub mod responder;
/// Invoices and payouts Rocket routes from `examples/rocket/routes_invoices_payouts.rs`.
#[path = "../../examples/rocket/routes_invoices_payouts.rs"]
pub mod routes_invoices_payouts;
/// Ledger transfers, internal transactions, sources, and Apple Pay routes from `examples/rocket/routes_ledger_sources.rs`.
#[path = "../../examples/rocket/routes_ledger_sources.rs"]
pub mod routes_ledger_sources;
/// Payments, payment intents, tokens, and card auths routes from `examples/rocket/routes_payments_tokens.rs`.
#[path = "../../examples/rocket/routes_payments_tokens.rs"]
pub mod routes_payments_tokens;
/// Settlements Rocket routes from `examples/rocket/routes_settlements.rs`.
#[path = "../../examples/rocket/routes_settlements.rs"]
pub mod routes_settlements;
/// Stripe compatibility and unified gateway routes from `examples/rocket/routes_stripe_unified.rs`.
#[path = "../../examples/rocket/routes_stripe_unified.rs"]
pub mod routes_stripe_unified;
/// Webhooks Rocket routes from `examples/rocket/routes_webhooks.rs`.
#[path = "../../examples/rocket/routes_webhooks.rs"]
pub mod routes_webhooks;
/// Managed Rocket application state from `examples/rocket/state.rs`.
#[path = "../../examples/rocket/state.rs"]
pub mod state;

mod invoices_payouts_test;
mod ledger_sources_test;
mod payments_tokens_test;
mod settlements_test;
mod stripe_unified_test;
mod webhooks_test;

/// Deterministic HTTP status and JSON body specification for mock endpoints.
pub struct MockResponseSpec {
    /// HTTP status code returned by the mock endpoint.
    pub status: u16,
    /// JSON response body returned by the mock endpoint.
    pub body: Value,
}

/// Builds a [`Rocket`] instance managed with [`state::RocketAppState`] and real example routes.
#[must_use]
pub fn build_test_rocket(client: MoyasarClient<SecretKey>) -> Rocket<Build> {
    let app_state = state::RocketAppState::new(
        client,
        ConstantTimeSecretVerifier::with_default_tolerance("whsec_test_secret"),
        InMemoryDeduplicator::new(),
    );
    rocket::build()
        .manage(app_state)
        .mount("/", routes_invoices_payouts::routes())
        .mount("/", routes_settlements::routes())
        .mount("/", routes_webhooks::routes())
        .mount("/", routes_payments_tokens::routes())
        .mount("/", routes_ledger_sources::routes())
        .mount("/", routes_stripe_unified::routes())
}

/// Spawns a [`MoyasarMockServer`] and a tracked Rocket [`Client`] wired to it.
pub async fn spawn_test_client() -> (MoyasarMockServer, Client) {
    let server = MoyasarMockServer::start().await;
    let sdk_client = MoyasarClient::builder(
        SecretKey::new("sk_test_rocket_e2e_12345").expect("valid test secret key"),
    )
    .base_url(server.uri())
    .timeout(Duration::from_millis(500))
    .max_network_retries(0)
    .build()
    .expect("loopback mock client should build");

    let rocket_client = Client::tracked(build_test_rocket(sdk_client))
        .await
        .expect("tracked Rocket client should initialize");

    (server, rocket_client)
}

/// Mounts a deterministic JSON response on [`MoyasarMockServer`] for `moyasar-version`.
pub async fn mount_json_endpoint(
    server: &MoyasarMockServer,
    http_method: &str,
    endpoint_path: &str,
    response: MockResponseSpec,
) {
    Mock::given(method(http_method))
        .and(path(endpoint_path))
        .and(header("moyasar-version", API_VERSION))
        .respond_with(ResponseTemplate::new(response.status).set_body_json(response.body))
        .mount(server.server())
        .await;
}
