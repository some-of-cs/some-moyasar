//! Payment intent route handlers (`POST /intents`, `GET /intents/{id}`, `POST /intents/{id}/cancel`).
#![forbid(unsafe_code)]

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use some_moyasar::prelude::{
    CreatePaymentIntentParams, Currency, IntentId, Metadata, PaymentIntent, PaymentIntentStatus,
};
use uuid::Uuid;

use super::error::AppError;
use super::state::AppState;

/// Request payload for `POST /intents` using integer minor-unit amounts.
#[derive(Debug, Deserialize)]
pub struct CreateIntentRequest {
    /// Payment amount in integer minor currency units (e.g., Halalas for SAR).
    pub amount: u64,
    /// ISO 4217 currency code.
    pub currency: Currency,
    /// Redirect callback URL after 3DS or payment completion.
    pub callback_url: String,
    /// Optional human-readable payment description.
    pub description: Option<String>,
    /// Optional client-supplied idempotency UUID.
    pub given_id: Option<Uuid>,
    /// Optional validated key/value metadata.
    pub metadata: Option<Metadata>,
}

/// Serializable JSON representation of a [`PaymentIntent`].
#[derive(Debug, Serialize)]
pub struct IntentResponse {
    /// Unique Moyasar Payment Intent identifier.
    pub id: IntentId,
    /// Current intent lifecycle status.
    pub status: PaymentIntentStatus,
    /// Locked payment amount in integer minor currency units.
    pub amount: u64,
    /// Human-readable formatted amount string returned by Moyasar, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_format: Option<String>,
    /// Locked ISO 4217 currency code.
    pub currency: Currency,
    /// Optional locked payment description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Locked redirect callback URL for 3DS or completion flows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_url: Option<String>,
    /// Ephemeral client secret used by the storefront to fulfill the intent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// Timestamp after which the intent can no longer be fulfilled or canceled.
    pub expires_at: DateTime<Utc>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

impl From<&PaymentIntent> for IntentResponse {
    fn from(intent: &PaymentIntent) -> Self {
        Self {
            id: intent.id.clone(),
            status: intent.status,
            amount: intent.amount,
            amount_format: intent.amount_format.clone(),
            currency: intent.currency,
            description: intent.description.clone(),
            callback_url: intent.callback_url.clone(),
            client_secret: intent.client_secret().map(str::to_owned),
            expires_at: intent.expires_at,
            created_at: intent.created_at,
            updated_at: intent.updated_at,
        }
    }
}

/// Builds the Axum [`Router`] for payment intent endpoints.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/intents", post(create_intent))
        .route("/intents/{id}", get(retrieve_intent))
        .route("/intents/{id}/cancel", post(cancel_intent))
}

fn build_create_params(req: CreateIntentRequest) -> CreatePaymentIntentParams {
    let mut params = CreatePaymentIntentParams::new(req.amount, req.currency, req.callback_url);
    if let Some(description) = req.description {
        params = params.description(description);
    }
    if let Some(given_id) = req.given_id {
        params = params.given_id(given_id);
    }
    if let Some(metadata) = req.metadata {
        params = params.metadata(metadata);
    }
    params
}

fn gone_response(intent: &PaymentIntent) -> Response {
    let body = json!({
        "error": "payment_intent_unusable",
        "message": "payment intent is expired, fulfilled, or canceled",
        "id": intent.id,
        "status": intent.status,
        "expires_at": intent.expires_at,
    });
    (StatusCode::GONE, Json(body)).into_response()
}

async fn create_intent(
    State(state): State<AppState>,
    Json(payload): Json<CreateIntentRequest>,
) -> Result<(StatusCode, Json<IntentResponse>), AppError> {
    let params = build_create_params(payload);
    let intent = state.client.payment_intents().create(params).await?;
    Ok((StatusCode::CREATED, Json(IntentResponse::from(&intent))))
}

async fn retrieve_intent(
    State(state): State<AppState>,
    Path(raw_id): Path<String>,
) -> Result<Response, AppError> {
    let id = IntentId::new(raw_id)?;
    let intent = state.client.payment_intents().retrieve(&id).await?;
    if !intent.is_usable() {
        return Ok(gone_response(&intent));
    }
    Ok((StatusCode::OK, Json(IntentResponse::from(&intent))).into_response())
}

async fn cancel_intent(
    State(state): State<AppState>,
    Path(raw_id): Path<String>,
) -> Result<Json<IntentResponse>, AppError> {
    let id = IntentId::new(raw_id)?;
    let intent = state.client.payment_intents().cancel(&id).await?;
    Ok(Json(IntentResponse::from(&intent)))
}
