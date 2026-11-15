//! Shared Axum E2E test router harness backed by `MoyasarMockServer`.
#![forbid(unsafe_code)]

#[path = "../../examples/axum/error.rs"]
pub mod error;
#[path = "../../examples/axum/routes_intents.rs"]
pub mod routes_intents;
#[path = "../../examples/axum/routes_payments.rs"]
pub mod routes_payments;
#[path = "../../examples/axum/routes_webhooks.rs"]
pub mod routes_webhooks;
#[path = "../../examples/axum/state.rs"]
pub mod state;

mod intents_test;
mod payments_test;
mod webhooks_test;

use std::sync::Arc;

use some_moyasar::prelude::{
    ConstantTimeSecretVerifier, InMemoryDeduplicator, MoyasarClient, SecretKey,
};
use some_moyasar::test_utils::MoyasarMockServer;

/// Deterministic webhook signing secret shared across Axum E2E tests.
pub const TEST_WEBHOOK_SECRET: &str = "whsec_test_axum_secret_key";

/// Starts a hermetic [`MoyasarMockServer`] and builds the merged Axum test [`axum::Router`].
pub async fn spawn_test_app() -> (MoyasarMockServer, axum::Router) {
    let mock = MoyasarMockServer::start().await;
    let key = SecretKey::new("sk_test_axum_key").expect("valid test secret key");
    let client = MoyasarClient::builder(key)
        .base_url(mock.uri())
        .build()
        .expect("loopback mock client builds");
    let verifier = Arc::new(ConstantTimeSecretVerifier::with_default_tolerance(
        TEST_WEBHOOK_SECRET,
    ));
    let deduplicator = Arc::new(InMemoryDeduplicator::new());
    let app_state = state::AppState::new(client, verifier, deduplicator);
    let router = axum::Router::new()
        .merge(routes_payments::router())
        .merge(routes_intents::router())
        .merge(routes_webhooks::router())
        .with_state(app_state);
    (mock, router)
}
