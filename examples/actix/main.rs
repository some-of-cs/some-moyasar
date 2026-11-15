#![forbid(unsafe_code)]
//! Actix-Web reference server for `some-moyasar`.

mod error;
mod handlers_payments;
mod handlers_subscriptions;
mod handlers_webhooks;
mod state;

use actix_web::{App, HttpServer, web};
use some_moyasar::prelude::MoyasarClient;

use crate::state::ActixAppState;

/// Configures all v1 payment, customer token vault, subscription, and webhook
/// routes on the given Actix-Web [`web::ServiceConfig`].
pub fn configure_app(cfg: &mut actix_web::web::ServiceConfig) {
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

/// Starts the Actix-Web HTTP server with shared [`ActixAppState`] and configured routes.
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let client = MoyasarClient::new("$MOYASAR_SECRET_KEY").map_err(std::io::Error::other)?;
    let webhook_secret = std::env::var("MOYASAR_WEBHOOK_SECRET")
        .unwrap_or_else(|_| "whsec_dev_placeholder".to_string());
    let state = ActixAppState::new(client, &webhook_secret);
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
