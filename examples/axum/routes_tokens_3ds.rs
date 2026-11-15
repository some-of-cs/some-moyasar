//! Tokens, 3DS card authentications, and invoices route handlers (stub).
#![forbid(unsafe_code)]

use axum::Router;
use super::state::AppState;

/// Builds the router for tokens, card authentications, and invoices.
pub fn router() -> Router<AppState> {
    Router::new()
}
