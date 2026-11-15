//! Stripe-compatible Customers, Subscriptions, and UnifiedPaymentGateway route handlers (stub).
#![forbid(unsafe_code)]

use axum::Router;
use super::state::AppState;

/// Builds the router for customers, subscriptions, and unified gateway endpoints.
pub fn router() -> Router<AppState> {
    Router::new()
}
