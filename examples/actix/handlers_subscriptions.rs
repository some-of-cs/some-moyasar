//! Actix-Web handlers for subscription cycle billing and pro-rata refunds.

use actix_web::{HttpResponse, web};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use some_moyasar::prelude::{
    Currency, RefundStrategy, SubscriptionCycleChargeParams, SubscriptionCycleRefundParams, TokenId,
};

use super::error::AppError;
use super::state::ActixAppState;

/// Request payload for charging a single subscription billing cycle.
#[derive(Debug, Clone, Deserialize)]
pub struct CycleChargeInput {
    /// Monotonic billing cycle index used to derive the deterministic UUIDv5 `given_id`.
    pub cycle_index: u64,
    /// Charge amount in integer minor units (e.g., halalas for SAR).
    pub amount: u64,
    /// Optional ISO 4217 currency (defaults to [`Currency::SAR`]).
    pub currency: Option<Currency>,
    /// Saved card token identifier (`token_...`).
    pub token_id: String,
}

/// Request payload for pro-rata refunding a canceled subscription billing cycle.
#[derive(Debug, Clone, Deserialize)]
pub struct CycleRefundInput {
    /// Moyasar payment ID associated with the charged cycle.
    pub cycle_payment_id: String,
    /// Start of the billing period (UTC).
    pub period_start: DateTime<Utc>,
    /// End of the billing period (UTC).
    pub period_end: DateTime<Utc>,
    /// UTC timestamp when the subscription was canceled.
    pub canceled_at: DateTime<Utc>,
}

/// Charges a subscription billing cycle off-session (`POST /v1/subscriptions/{sub_id}/charge`).
///
/// Derives a deterministic UUIDv5 `given_id` from `sub_id` and `cycle_index`,
/// validates the saved `token_id`, and returns `201 Created` with the resulting payment JSON.
///
/// # Errors
///
/// Returns [`AppError`] if `token_id` is invalid, the token is inactive, or the Moyasar API
/// request fails.
pub async fn charge_subscription_cycle(
    state: web::Data<ActixAppState>,
    path: web::Path<String>,
    body: web::Json<CycleChargeInput>,
) -> Result<HttpResponse, AppError> {
    let sub_id = path.into_inner();
    let token_id = TokenId::new(&body.token_id)?;
    let params =
        SubscriptionCycleChargeParams::new(&sub_id, body.cycle_index, body.amount, &token_id)
            .currency(body.currency.unwrap_or(Currency::SAR));
    let payment = state.client.subscriptions().charge_cycle(params).await?;
    Ok(HttpResponse::Created().json(payment))
}

/// Refunds the unused portion of a subscription cycle (`POST /v1/subscriptions/{sub_id}/refund`).
///
/// Computes the pro-rata refund using integer minor-unit math via [`RefundStrategy::ProRata`]
/// and returns `200 OK` with the reversal result JSON.
///
/// # Errors
///
/// Returns [`AppError`] if the payment ID or period bounds are invalid, or if the Moyasar API
/// request fails.
pub async fn refund_subscription_cycle(
    state: web::Data<ActixAppState>,
    path: web::Path<String>,
    body: web::Json<CycleRefundInput>,
) -> Result<HttpResponse, AppError> {
    let sub_id = path.into_inner();
    let params = SubscriptionCycleRefundParams {
        subscription_id: sub_id,
        cycle_payment_id: body.cycle_payment_id.clone(),
        period_start: body.period_start,
        period_end: body.period_end,
        strategy: RefundStrategy::ProRata {
            canceled_at: body.canceled_at,
        },
    };
    let result = state.client.subscriptions().refund_cycle(&params).await?;
    Ok(HttpResponse::Ok().json(result))
}
