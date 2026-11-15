//! Payment & refund route handlers for the Axum reference server.
#![forbid(unsafe_code)]

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use some_moyasar::prelude::{
    CreatePaymentParams, Currency, MoyasarError, Payment, PaymentId, PaymentStatus,
    RefundCreateParams, RefundReceipt,
};

use crate::error::AppError;
use crate::state::AppState;

/// Request payload for `POST /payments`.
///
/// Uses integer minor-unit `amount` (`u64`) and an optional `given_id` UUID for
/// idempotent payment creation. The `source` field accepts the JSON
/// representation of [`some_moyasar::prelude::PaymentSourceRequest`].
#[derive(Debug, Deserialize)]
pub struct CreatePaymentRequest {
    /// Payment amount in the currency's smallest minor unit (e.g., Halalas for SAR).
    pub amount: u64,
    /// ISO 4217 currency code.
    pub currency: Currency,
    /// Polymorphic payment source payload matching `PaymentSourceRequest`.
    pub source: Value,
    /// Optional client-supplied UUIDv4/v5 idempotency key (`given_id`).
    pub given_id: Option<Uuid>,
    /// Optional human-readable payment description.
    pub description: Option<String>,
    /// Optional 3DS / redirect callback URL.
    pub callback_url: Option<String>,
}

/// Request payload for `POST /payments/{id}/refund`.
#[derive(Debug, Default, Deserialize)]
pub struct RefundPaymentRequest {
    /// Optional partial refund amount in minor units; `None` reverses the full amount.
    pub amount: Option<u64>,
}

/// Redacted JSON response DTO for [`Payment`].
#[derive(Debug, Serialize)]
pub struct PaymentResponse {
    /// Unique Moyasar payment identifier.
    pub id: String,
    /// Current lifecycle status of the payment.
    pub status: PaymentStatus,
    /// Payment amount in minor units.
    pub amount: u64,
    /// Pre-formatted human-readable payment amount string.
    pub amount_format: String,
    /// ISO 4217 currency code.
    pub currency: Currency,
    /// Optional merchant-supplied description.
    pub description: Option<String>,
    /// Optional 3DS / redirect callback URL.
    pub callback_url: Option<String>,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last-update timestamp.
    pub updated_at: String,
}

impl From<Payment> for PaymentResponse {
    fn from(payment: Payment) -> Self {
        Self {
            id: payment.id,
            status: payment.status,
            amount: payment.amount,
            amount_format: payment.amount_format,
            currency: payment.currency,
            description: payment.description,
            callback_url: payment.callback_url,
            created_at: payment.created_at.to_rfc3339(),
            updated_at: payment.updated_at.to_rfc3339(),
        }
    }
}

/// Builds the Axum router for payment creation, retrieval, and smart refund/void routes.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/payments", post(create_payment))
        .route("/payments/{id}", get(retrieve_payment))
        .route("/payments/{id}/refund", post(refund_payment))
}

fn build_create_params(payload: CreatePaymentRequest) -> Result<CreatePaymentParams, MoyasarError> {
    let mut builder = CreatePaymentParams::builder()
        .amount(payload.amount)
        .currency(payload.currency)
        .source(payload.source);

    if let Some(given_id) = payload.given_id {
        builder = builder.given_id(given_id);
    }
    if let Some(description) = payload.description {
        builder = builder.description(description);
    }
    if let Some(callback_url) = payload.callback_url {
        builder = builder.callback_url(callback_url);
    }

    builder.build()
}

/// Handles `POST /payments` with `given_id` idempotency.
async fn create_payment(
    State(state): State<AppState>,
    Json(payload): Json<CreatePaymentRequest>,
) -> Result<(StatusCode, Json<PaymentResponse>), AppError> {
    let params = build_create_params(payload)?;
    let payment = state.client.payments().create(&params).await?;
    Ok((StatusCode::CREATED, Json(PaymentResponse::from(payment))))
}

/// Handles `GET /payments/{id}` by retrieving a payment by its `PaymentId`.
async fn retrieve_payment(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PaymentResponse>, AppError> {
    let payment_id = PaymentId::new(id)?;
    let payment = state.client.payments().retrieve(&payment_id).await?;
    Ok(Json(PaymentResponse::from(payment)))
}

/// Handles `POST /payments/{id}/refund` with smart 2-hour unsettled void vs refund routing.
async fn refund_payment(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<RefundPaymentRequest>,
) -> Result<Json<RefundReceipt>, AppError> {
    let mut params = RefundCreateParams::new(id).prefer_void_if_unsettled(true);
    if let Some(amount) = payload.amount {
        params = params.amount(amount);
    }
    let receipt = state.client.refunds().create(params).await?;
    Ok(Json(receipt))
}
