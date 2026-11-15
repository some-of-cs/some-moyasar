//! Rocket.rs integration example server for `some-moyasar`.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use some_moyasar::prelude::{
    ConstantTimeSecretVerifier, InMemoryDeduplicator, MoyasarClient,
};

use crate::state::RocketAppState;

pub mod responder;
pub mod routes_invoices_payouts;
pub mod routes_settlements;
pub mod routes_webhooks;
pub mod state;

/// Builds the Rocket application instance with managed [`RocketAppState`] and all
/// Moyasar integration routes mounted under `/`.
#[must_use]
pub fn build_rocket(state: RocketAppState) -> rocket::Rocket<rocket::Build> {
    let mut routes = routes_invoices_payouts::routes();
    routes.extend(routes_settlements::routes());
    routes.extend(routes_webhooks::routes());

    rocket::build().manage(state).mount("/", routes)
}

/// Initializes the Moyasar SDK client, webhook verifier, and in-memory deduplicator
/// from environment variables and launches the Rocket HTTP server.
#[rocket::main]
async fn main() -> Result<(), Box<rocket::Error>> {
    let client = MoyasarClient::new("$MOYASAR_SECRET_KEY")
        .expect("MOYASAR_SECRET_KEY must be set to a valid Moyasar secret key");
    let webhook_verifier =
        ConstantTimeSecretVerifier::with_default_tolerance("$MOYASAR_WEBHOOK_SECRET");
    let deduplicator = Arc::new(InMemoryDeduplicator::new());

    let state = RocketAppState {
        client,
        webhook_verifier,
        deduplicator,
    };

    build_rocket(state).launch().await.map_err(Box::new)?;
    Ok(())
}
