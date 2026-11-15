//! Rocket webhook ingestion route with Double-HMAC verification and deduplication.

use rocket::State;
use rocket::data::ToByteUnit;
use rocket::serde::json::Json;
use serde::Serialize;
use uuid::Uuid;

use some_moyasar::prelude::{MoyasarError, WebhookError};

use crate::responder::MoyasarResponder;
use crate::state::RocketAppState;

/// Maximum permitted inbound Moyasar webhook payload size (64 KiB).
const MAX_WEBHOOK_BODY_KIB: u64 = 64;

/// JSON acknowledgement returned when an inbound Moyasar webhook is verified and accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WebhookAckResponse {
    /// Processing status (`"accepted"`).
    pub status: &'static str,
    /// Unique identifier of the verified webhook event.
    pub event_id: Uuid,
    /// Canonical Moyasar wire-format event type (e.g., `"payment_paid"`).
    pub event_type: String,
}

/// Maps a [`WebhookError`] from verification or deduplication into a [`MoyasarError`].
fn map_webhook_error(err: WebhookError) -> MoyasarError {
    match err {
        WebhookError::SecretMismatch => MoyasarError::InvalidWebhookSecret,
        WebhookError::TimestampOutOfRange => MoyasarError::InvalidWebhookSignature,
        WebhookError::InvalidJson(json_err) => {
            MoyasarError::invalid_request(format!("invalid webhook JSON: {json_err}"))
        }
        WebhookError::DuplicateEvent(event_id) => {
            MoyasarError::invalid_request(format!("duplicate webhook event: {event_id}"))
        }
    }
}

/// Verifies an inbound Moyasar webhook payload, enforces replay deduplication,
/// and acknowledges receipt.
///
/// # Errors
///
/// Returns [`MoyasarResponder`] wrapping:
/// - [`MoyasarError::InvalidWebhookSecret`] when `secret_token` fails constant-time verification.
/// - [`MoyasarError::InvalidWebhookSignature`] when `created_at` falls outside replay tolerance.
/// - [`MoyasarError`] (`InvalidRequestError`) on oversized/unreadable bodies, malformed JSON,
///   or duplicate `event.id` deliveries.
#[rocket::post("/webhooks/moyasar", data = "<body>")]
pub async fn handle_webhook(
    state: &State<RocketAppState>,
    body: rocket::Data<'_>,
) -> Result<Json<WebhookAckResponse>, MoyasarResponder> {
    let capped = body
        .open(MAX_WEBHOOK_BODY_KIB.kibibytes())
        .into_bytes()
        .await
        .map_err(|err| {
            MoyasarResponder(MoyasarError::invalid_request(format!(
                "failed to read webhook body: {err}"
            )))
        })?;

    if !capped.is_complete() {
        return Err(MoyasarResponder(MoyasarError::invalid_request(
            "webhook payload exceeds 64 KiB limit",
        )));
    }

    let bytes = capped.into_inner();
    let event = state
        .webhook_verifier
        .verify_and_construct(&bytes)
        .map_err(map_webhook_error)
        .map_err(MoyasarResponder)?;

    let is_fresh = state
        .deduplicator
        .check_and_record(event.id)
        .await
        .map_err(map_webhook_error)
        .map_err(MoyasarResponder)?;

    if !is_fresh {
        return Err(MoyasarResponder(MoyasarError::invalid_request(
            "duplicate webhook event",
        )));
    }

    Ok(Json(WebhookAckResponse {
        status: "accepted",
        event_id: event.id,
        event_type: event.event_type.as_str().to_owned(),
    }))
}

/// Returns all Rocket routes for Moyasar webhook ingestion.
#[must_use]
pub fn routes() -> Vec<rocket::Route> {
    rocket::routes![handle_webhook]
}
