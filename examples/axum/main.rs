//! Axum reference server integrating `some-moyasar`.
#![forbid(unsafe_code)]

mod error;
mod routes_intents;
mod routes_payments;
mod routes_webhooks;
mod state;

#[tokio::main]
async fn main() {
    // Wired in US-AXUM-1.0 Unit 4.
}
