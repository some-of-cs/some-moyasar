//! Constant-time webhook verification, deduplication, and `WebhooksService` handlers for Actix-Web.

use actix_web::{HttpResponse, web};
use serde::Deserialize;
use serde_json::json;
use some_moyasar::prelude::{
    CreateWebhookParams, WebhookDeduplicator, WebhookError, WebhookEventType,
};

use super::error::AppError;
use super::state::ActixAppState;

/// Request payload for `POST /v1/webhooks/endpoints`.
#[derive(Debug, Deserialize)]
pub struct CreateWebhookEndpointInput {
    /// HTTPS destination URL for outbound Moyasar webhook deliveries.
    pub url: String,
    /// Shared secret configured for the webhook endpoint.
    pub shared_secret: String,
    /// Optional list of subscribed [`WebhookEventType`] filters.
    pub events: Option<Vec<WebhookEventType>>,
}

/// Registers all inbound webhook and `WebhooksService` management routes on `cfg`.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/v1/webhooks", web::post().to(handle_webhook))
        .route(
            "/v1/webhooks/endpoints",
            web::post().to(create_webhook_endpoint),
        )
        .route(
            "/v1/webhooks/endpoints",
            web::get().to(list_webhook_endpoints),
        )
        .route(
            "/v1/webhooks/available_events",
            web::get().to(list_available_webhook_events),
        )
        .route(
            "/v1/webhooks/attempts",
            web::get().to(list_webhook_attempts),
        );
}

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

/// Handles `POST /v1/webhooks/endpoints` via `WebhooksService::create`.
pub async fn create_webhook_endpoint(
    state: web::Data<ActixAppState>,
    body: web::Json<CreateWebhookEndpointInput>,
) -> Result<HttpResponse, AppError> {
    let mut params = CreateWebhookParams::new(&body.url, &body.shared_secret)?;
    if let Some(ref events) = body.events {
        params = params.events(events.clone());
    }
    let endpoint = state.client.webhooks().create(&params).await?;
    Ok(HttpResponse::Created().json(endpoint))
}

/// Handles `GET /v1/webhooks/endpoints` via `WebhooksService::list`.
pub async fn list_webhook_endpoints(
    state: web::Data<ActixAppState>,
) -> Result<HttpResponse, AppError> {
    let page = state.client.webhooks().list(None).await?;
    Ok(HttpResponse::Ok().json(json!({
        "items": page.items,
        "total_count": page.meta.total_count,
    })))
}

/// Handles `GET /v1/webhooks/available_events` via `WebhooksService::available_events`.
pub async fn list_available_webhook_events(
    state: web::Data<ActixAppState>,
) -> Result<HttpResponse, AppError> {
    let events = state.client.webhooks().available_events().await?;
    Ok(HttpResponse::Ok().json(json!({ "available_events": events })))
}

/// Handles `GET /v1/webhooks/attempts` via `WebhooksService::list_attempts`.
pub async fn list_webhook_attempts(
    state: web::Data<ActixAppState>,
) -> Result<HttpResponse, AppError> {
    let page = state.client.webhooks().list_attempts(None).await?;
    Ok(HttpResponse::Ok().json(json!({
        "items": page.items,
        "total_count": page.meta.total_count,
    })))
}
