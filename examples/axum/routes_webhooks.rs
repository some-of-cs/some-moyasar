//! Moyasar webhook route handler with constant-time secret verification,
//! replay-window timestamp validation, and idempotent deduplication.
#![forbid(unsafe_code)]

use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use chrono::Utc;
use serde_json::json;
use some_moyasar::prelude::WebhookDeduplicator;
use some_moyasar::webhook_engine::{WebhookError, WebhookEvent, validate_timestamp_at};

use crate::state::AppState;

/// Builds the webhook [`Router`] mounted at `POST /webhooks/moyasar`.
pub fn router() -> Router<AppState> {
    Router::new().route("/webhooks/moyasar", post(handle_moyasar_webhook))
}

/// Builds a JSON error [`Response`] with the given HTTP status and message.
fn error_response(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

/// Verifies the raw webhook payload signature and validates its replay timestamp.
fn verify_and_validate_webhook(
    state: &AppState,
    body: &[u8],
) -> Result<WebhookEvent, (StatusCode, &'static str)> {
    let event = match state.verifier.verify_and_construct(body) {
        Ok(event) => event,
        Err(WebhookError::InvalidJson(_)) => {
            return Err((StatusCode::BAD_REQUEST, "invalid webhook payload"));
        }
        Err(_) => {
            return Err((StatusCode::UNAUTHORIZED, "unauthorized webhook"));
        }
    };

    if validate_timestamp_at(event.created_at, Utc::now(), state.verifier.tolerance()).is_err() {
        return Err((
            StatusCode::UNAUTHORIZED,
            "stale or invalid webhook timestamp",
        ));
    }

    Ok(event)
}

/// Handles inbound `POST /webhooks/moyasar` notifications.
///
/// Extracts the raw [`Bytes`] body, performs constant-time Double-HMAC secret
/// verification, validates the `created_at` replay window, and deduplicates by
/// `event.id`.
async fn handle_moyasar_webhook(State(state): State<AppState>, body: Bytes) -> Response {
    let event = match verify_and_validate_webhook(&state, &body) {
        Ok(event) => event,
        Err((status, msg)) => return error_response(status, msg),
    };

    let is_fresh = match state.deduplicator.check_and_record(event.id).await {
        Ok(fresh) => fresh,
        Err(_) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "webhook deduplication failed",
            );
        }
    };

    if !is_fresh {
        return (StatusCode::OK, Json(json!({ "status": "duplicate" }))).into_response();
    }

    (
        StatusCode::OK,
        Json(json!({
            "status": "processed",
            "event_id": event.id,
        })),
    )
        .into_response()
}
