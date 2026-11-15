//! Stripe-compatible `CustomersService`, `SubscriptionsService`, and `UnifiedPaymentGateway` route handlers.
#![forbid(unsafe_code)]

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use some_moyasar::prelude::{
    Currency, CustomerChargeParams, Metadata, Payment, PaymentStatus, RefundStrategy,
    SubscriptionCycleChargeParams, SubscriptionCycleRefundParams, SubscriptionCycleRefundResult,
    TokenId, UnifiedChargeResponse, UnifiedReversalResponse, UnifiedTokenChargeRequest,
};
use some_moyasar::unified::UnifiedPaymentGateway;

use super::error::AppError;
use super::state::AppState;

/// Request payload for `POST /customers/{id}/tokens`.
#[derive(Debug, Deserialize)]
pub struct AttachCustomerTokenRequest {
    /// Saved card token identifier (`token_...`) to associate with the customer in the vault.
    pub token_id: String,
}

/// JSON response payload for `POST /customers/{id}/tokens`.
#[derive(Debug, Serialize)]
pub struct AttachCustomerTokenResponse {
    /// Merchant customer identifier bound in the vault.
    pub customer_id: String,
    /// Attached saved card token identifier (`token_...`).
    pub token_id: String,
    /// Attachment confirmation status (`"attached"`).
    pub status: &'static str,
}

/// Request payload for `POST /customers/{id}/charge`.
#[derive(Debug, Deserialize)]
pub struct ChargeCustomerRequest {
    /// Charge amount in minor units (e.g., Halalas for SAR).
    pub amount: u64,
    /// ISO 4217 currency code.
    pub currency: Currency,
    /// Caller-supplied `given_id` idempotency UUID for the purchase attempt.
    pub given_id: Uuid,
    /// Optional merchant metadata (will have `customer_id` injected automatically).
    pub metadata: Option<Metadata>,
}

/// Request payload for `POST /subscriptions/{id}/charge`.
#[derive(Debug, Deserialize)]
pub struct ChargeSubscriptionCycleRequest {
    /// Monotonic billing cycle index.
    pub cycle_index: u64,
    /// Minor-unit cycle charge amount.
    pub amount: u64,
    /// Optional ISO 4217 currency code (defaults to [`Currency::SAR`]).
    pub currency: Option<Currency>,
    /// Saved card token identifier (`token_...`).
    pub token_id: String,
}

/// Request payload for `POST /subscriptions/{id}/refund`.
#[derive(Debug, Deserialize)]
pub struct RefundSubscriptionCycleRequest {
    /// Moyasar payment ID associated with the cycle charge.
    pub cycle_payment_id: String,
    /// Start of the billing period (UTC).
    pub period_start: DateTime<Utc>,
    /// End of the billing period (UTC).
    pub period_end: DateTime<Utc>,
    /// Reversal strategy (`Full`, `Partial`, or `ProRata`).
    pub strategy: RefundStrategy,
}

/// Request payload for `POST /unified/charge_token`.
#[derive(Debug, Deserialize)]
pub struct UnifiedChargeTokenPayload {
    /// Charge amount in minor units (`u64`).
    pub amount_minor: u64,
    /// ISO 4217 currency code string (e.g., `"SAR"`).
    pub currency: String,
    /// Saved card token identifier (`"token_..."`).
    pub token_id: String,
    /// Deterministic or caller-supplied UUID idempotency key (`given_id`).
    pub idempotency_key: Uuid,
}

/// Request payload for `POST /unified/reversal`.
#[derive(Debug, Deserialize)]
pub struct UnifiedReversalPayload {
    /// Gateway transaction/payment identifier (`pay_...`).
    pub transaction_id: String,
    /// Optional partial reversal amount in minor units (`None` reverses full amount).
    pub amount_minor: Option<u64>,
}

/// Redacted JSON response DTO for customer and subscription [`Payment`] charges.
#[derive(Debug, Serialize)]
pub struct StripePaymentResponse {
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
    /// Validated merchant key/value metadata (including injected `customer_id`).
    pub metadata: Metadata,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last-update timestamp.
    pub updated_at: String,
}

impl From<Payment> for StripePaymentResponse {
    fn from(payment: Payment) -> Self {
        Self {
            id: payment.id,
            status: payment.status,
            amount: payment.amount,
            amount_format: payment.amount_format,
            currency: payment.currency,
            metadata: payment.metadata,
            created_at: payment.created_at.to_rfc3339(),
            updated_at: payment.updated_at.to_rfc3339(),
        }
    }
}

/// Builds the router for `CustomersService`, `SubscriptionsService`, and `UnifiedPaymentGateway` endpoints.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/customers/{id}/tokens", post(attach_customer_token))
        .route("/customers/{id}/charge", post(charge_customer))
        .route(
            "/subscriptions/{id}/charge",
            post(charge_subscription_cycle),
        )
        .route(
            "/subscriptions/{id}/refund",
            post(refund_subscription_cycle),
        )
        .route("/unified/charge_token", post(unified_charge_token))
        .route("/unified/reversal", post(unified_reversal))
}

/// Builds [`CustomerChargeParams`] from the path customer ID and request payload.
fn build_customer_charge_params(
    customer_id: &str,
    payload: ChargeCustomerRequest,
) -> CustomerChargeParams<'_> {
    let mut params = CustomerChargeParams::new(customer_id, payload.amount, payload.currency)
        .given_id(payload.given_id);
    if let Some(metadata) = payload.metadata {
        params = params.metadata(metadata);
    }
    params
}

/// Handles `POST /customers/{id}/tokens` via [`some_moyasar::prelude::CustomersService::attach_token`].
async fn attach_customer_token(
    State(state): State<AppState>,
    Path(customer_id): Path<String>,
    Json(payload): Json<AttachCustomerTokenRequest>,
) -> Result<(StatusCode, Json<AttachCustomerTokenResponse>), AppError> {
    let token_id = TokenId::new(payload.token_id)?;
    let token_str = token_id.as_str().to_owned();
    state
        .client
        .customers(state.vault.as_ref())
        .attach_token(&customer_id, token_id)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(AttachCustomerTokenResponse {
            customer_id,
            token_id: token_str,
            status: "attached",
        }),
    ))
}

/// Handles `POST /customers/{id}/charge` via [`some_moyasar::prelude::CustomersService::charge`].
async fn charge_customer(
    State(state): State<AppState>,
    Path(customer_id): Path<String>,
    Json(payload): Json<ChargeCustomerRequest>,
) -> Result<(StatusCode, Json<StripePaymentResponse>), AppError> {
    let params = build_customer_charge_params(&customer_id, payload);
    let payment = state
        .client
        .customers(state.vault.as_ref())
        .charge(&params)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(StripePaymentResponse::from(payment)),
    ))
}

/// Handles `POST /subscriptions/{id}/charge` via [`some_moyasar::prelude::SubscriptionsService::charge_cycle`].
async fn charge_subscription_cycle(
    State(state): State<AppState>,
    Path(subscription_id): Path<String>,
    Json(payload): Json<ChargeSubscriptionCycleRequest>,
) -> Result<(StatusCode, Json<StripePaymentResponse>), AppError> {
    let token_id = TokenId::new(payload.token_id)?;
    let currency = payload.currency.unwrap_or(Currency::SAR);
    let params = SubscriptionCycleChargeParams::new(
        &subscription_id,
        payload.cycle_index,
        payload.amount,
        &token_id,
    )
    .currency(currency);
    let payment = state.client.subscriptions().charge_cycle(&params).await?;
    Ok((
        StatusCode::CREATED,
        Json(StripePaymentResponse::from(payment)),
    ))
}

/// Handles `POST /subscriptions/{id}/refund` via [`some_moyasar::prelude::SubscriptionsService::refund_cycle`].
async fn refund_subscription_cycle(
    State(state): State<AppState>,
    Path(subscription_id): Path<String>,
    Json(payload): Json<RefundSubscriptionCycleRequest>,
) -> Result<Json<SubscriptionCycleRefundResult>, AppError> {
    let params = SubscriptionCycleRefundParams {
        subscription_id,
        cycle_payment_id: payload.cycle_payment_id,
        period_start: payload.period_start,
        period_end: payload.period_end,
        strategy: payload.strategy,
    };
    let result = state.client.subscriptions().refund_cycle(&params).await?;
    Ok(Json(result))
}

/// Handles `POST /unified/charge_token` via [`UnifiedPaymentGateway::charge_saved_token`].
async fn unified_charge_token(
    State(state): State<AppState>,
    Json(payload): Json<UnifiedChargeTokenPayload>,
) -> Result<(StatusCode, Json<UnifiedChargeResponse>), AppError> {
    let req = UnifiedTokenChargeRequest::new(
        payload.amount_minor,
        &payload.currency,
        &payload.token_id,
        payload.idempotency_key,
    );
    let response = state.client.charge_saved_token(req).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

/// Handles `POST /unified/reversal` via [`UnifiedPaymentGateway::refund_or_void`].
async fn unified_reversal(
    State(state): State<AppState>,
    Json(payload): Json<UnifiedReversalPayload>,
) -> Result<Json<UnifiedReversalResponse>, AppError> {
    let response = state
        .client
        .refund_or_void(&payload.transaction_id, payload.amount_minor)
        .await?;
    Ok(Json(response))
}
