//! Constant-time webhook verification and deduplication handler for Actix-Web.

use actix_web::{HttpResponse, web};
use serde_json::json;
use some_moyasar::prelude::{WebhookDeduplicator, WebhookError};

use super::state::ActixAppState;

/// Maps a [`WebhookError`] produced during signature or payload verification
/// into a redacted [`HttpResponse`] without leaking secrets or raw payloads.
fn map_verify_error(err: WebhookError) -> HttpResponse {
    match err {
        WebhookError::SecretMismatch => {
            HttpResponse::Unauthorized().json(json!({"error": "invalid webhook secret"}))
        }
        WebhookError::TimestampOutOfRange => HttpResponse::BadRequest()
            .json(json!({"error": "webhook timestamp outside replay window"})),
        WebhookError::InvalidJson(_) => {
            HttpResponse::BadRequest().json(json!({"error": "invalid webhook payload"}))
        }
        WebhookError::DuplicateEvent(_) => HttpResponse::Ok().json(json!({"status": "duplicate"})),
    }
}

/// Handles `POST /v1/webhooks` using constant-time Double-HMAC secret verification,
/// replay-window validation, and idempotent event deduplication.
pub async fn handle_webhook(state: web::Data<ActixAppState>, body: web::Bytes) -> HttpResponse {
    let event = match state.verifier.verify_and_construct(&body) {
        Ok(event) => event,
        Err(err) => return map_verify_error(err),
    };

    match state.deduplicator.check_and_record(event.id).await {
        Ok(false) => HttpResponse::Ok().json(json!({
            "status": "duplicate",
            "event_id": event.id,
        })),
        Ok(true) => HttpResponse::Ok().json(json!({
            "status": "processed",
            "event_id": event.id,
            "event_type": event.event_type.as_str(),
        })),
        Err(_) => HttpResponse::InternalServerError()
            .json(json!({"error": "webhook deduplication failed"})),
    }
}
