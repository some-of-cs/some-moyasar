//! Sources (BIN lookup), Apple Pay session initiation, and Webhooks CRUD API route handlers (stub).
#![forbid(unsafe_code)]

use axum::Router;
use super::state::AppState;

/// Builds the router for sources, Apple Pay, and webhooks management API.
pub fn router() -> Router<AppState> {
    Router::new()
}
