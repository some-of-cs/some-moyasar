#![forbid(unsafe_code)]
//! Actix-Web reference server for `some-moyasar`.

mod error;
mod handlers_payments;
mod handlers_subscriptions;
mod handlers_webhooks;
mod state;

use actix_web::{App, HttpServer, web};
use some_moyasar::prelude::{MoyasarClient, PublishableKey};

use crate::state::ActixAppState;

/// Configures all `some-moyasar` v1 payment, token, intent, invoice, 3DS card-auth,
/// customer vault, subscription, payout, settlement, ledger, source, Apple Pay,
/// unified gateway, and webhook routes on the given Actix-Web [`web::ServiceConfig`].
pub fn configure_app(cfg: &mut actix_web::web::ServiceConfig) {
    handlers_payments::configure(cfg);
    handlers_subscriptions::configure(cfg);
    handlers_webhooks::configure(cfg);
}

/// Starts the Actix-Web HTTP server with shared [`ActixAppState`] and configured routes.
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let client = MoyasarClient::new("$MOYASAR_SECRET_KEY").map_err(std::io::Error::other)?;
    let mut state = ActixAppState::new(client, "$MOYASAR_WEBHOOK_SECRET");
    if let Ok(pk) = PublishableKey::new("$MOYASAR_PUBLISHABLE_KEY")
        && let Ok(pub_client) = MoyasarClient::new(pk)
    {
        state = state.with_publishable_client(pub_client);
    }
    let bind_addr =
        std::env::var("ACTIX_LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(configure_app)
    })
    .bind(bind_addr)?
    .run()
    .await
}
