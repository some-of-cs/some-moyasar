//! Actix-Web integration test harness wired to `MoyasarMockServer`.

#[path = "../../examples/actix/state.rs"]
pub mod state;
#[path = "../../examples/actix/error.rs"]
pub mod error;
#[path = "../../examples/actix/handlers_payments.rs"]
pub mod handlers_payments;
#[path = "../../examples/actix/handlers_subscriptions.rs"]
pub mod handlers_subscriptions;
#[path = "../../examples/actix/handlers_webhooks.rs"]
pub mod handlers_webhooks;

mod payments_vault_test;
mod subscriptions_test;
mod webhooks_test;

use actix_web::web;
use some_moyasar::prelude::{MoyasarClient, MoyasarMockServer, SecretKey};

/// Test webhook secret used by the Actix-Web E2E test harness.
const TEST_WEBHOOK_SECRET: &str = "whsec_actix_test_secret";

/// Registers all Actix-Web example routes on the given [`web::ServiceConfig`].
pub fn configure_test_routes(cfg: &mut web::ServiceConfig) {
    cfg.route(
        "/v1/payments",
        web::post().to(handlers_payments::create_payment),
    )
    .route(
        "/v1/payments/{id}",
        web::get().to(handlers_payments::get_payment),
    )
    .route(
        "/v1/customers/{customer_id}/tokens",
        web::post().to(handlers_payments::attach_customer_token),
    )
    .route(
        "/v1/subscriptions/{sub_id}/charge",
        web::post().to(handlers_subscriptions::charge_subscription_cycle),
    )
    .route(
        "/v1/subscriptions/{sub_id}/refund",
        web::post().to(handlers_subscriptions::refund_subscription_cycle),
    )
    .route(
        "/v1/webhooks",
        web::post().to(handlers_webhooks::handle_webhook),
    );
}

/// Starts a hermetic [`MoyasarMockServer`] with default routes and builds shared [`state::ActixAppState`].
pub async fn build_test_harness() -> (MoyasarMockServer, web::Data<state::ActixAppState>) {
    let server = MoyasarMockServer::start().await;
    server.mount_default_routes().await;
    let key = SecretKey::new("sk_test_actix_harness_key").expect("valid test secret key");
    let client = MoyasarClient::builder(key)
        .base_url(server.uri())
        .build()
        .expect("loopback mock server URL should build MoyasarClient");
    let app_state = web::Data::new(state::ActixAppState::new(client, TEST_WEBHOOK_SECRET));
    (server, app_state)
}
