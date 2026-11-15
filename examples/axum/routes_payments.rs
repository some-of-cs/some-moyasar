//! Payment & refund route handlers for the Axum reference server.
#![forbid(unsafe_code)]

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use some_moyasar::prelude::{
    CreatePaymentParams, Currency, MoyasarError, Payment, PaymentId, RefundCreateParams,
    RefundReceipt,
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
) -> Result<(StatusCode, Json<Payment>), AppError> {
    let params = build_create_params(payload)?;
    let payment = state.client.payments().create(&params).await?;
    Ok((StatusCode::CREATED, Json(payment)))
}

/// Handles `GET /payments/{id}` by retrieving a payment by its `PaymentId`.
async fn retrieve_payment(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Payment>, AppError> {
    let payment_id = PaymentId::new(id)?;
    let payment = state.client.payments().retrieve(&payment_id).await?;
    Ok(Json(payment))
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
