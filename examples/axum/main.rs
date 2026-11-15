//! Axum reference server integrating all `some-moyasar` crate services.
#![forbid(unsafe_code)]

mod error;
mod routes_intents;
mod routes_payments;
mod routes_payouts_ledger;
mod routes_sources_webhooks_api;
mod routes_stripe_unified;
mod routes_tokens_3ds;
mod routes_webhooks;
mod state;

/// Assembles the Axum [`axum::Router`] by merging all service route groups
/// with shared [`state::AppState`].
pub fn build_app(state: state::AppState) -> axum::Router {
    axum::Router::new()
        .merge(routes_payments::router())
        .merge(routes_intents::router())
        .merge(routes_tokens_3ds::router())
        .merge(routes_payouts_ledger::router())
        .merge(routes_sources_webhooks_api::router())
        .merge(routes_stripe_unified::router())
        .merge(routes_webhooks::router())
        .with_state(state)
}

/// Entrypoint for the Axum reference server.
///
/// Builds [`state::AppState`] from environment variables and serves the
/// assembled router on `0.0.0.0:3000`.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let state = state::AppState::from_env()?;
    let app = build_app(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;
    Ok(())
}
