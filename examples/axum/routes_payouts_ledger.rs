//! Payout accounts, payouts, settlements, transfers, and internal transactions route handlers (stub).
#![forbid(unsafe_code)]

use axum::Router;
use super::state::AppState;

/// Builds the router for payouts and ledger/settlements services.
pub fn router() -> Router<AppState> {
    Router::new()
}
