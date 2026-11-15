//! Rocket routes for `RefundsService`, `CustomersService`, `SubscriptionsService`, and
//! `UnifiedPaymentGateway`.

use rocket::http::Status;
use rocket::response::status;
use rocket::serde::json::Json;
use rocket::{Route, State};
use serde::{Deserialize, Serialize};
use some_moyasar::prelude::{
    Currency, CustomerChargeParams, Payment, PaymentStatus, RefundCreateParams, RefundReceipt,
    SubscriptionCycleChargeParams, TokenId, UnifiedChargeResponse, UnifiedIntentRequest,
    UnifiedIntentResponse, UnifiedPaymentGateway, UnifiedReversalResponse,
    UnifiedTokenChargeRequest,
};
use uuid::Uuid;

use crate::responder::MoyasarResponder;
use crate::state::RocketAppState;

/// Rocket error responder alias for Stripe-compatible and unified gateway routes.
pub type AppError = MoyasarResponder;

/// Request payload for attaching a saved card token to a customer in the vault.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachCustomerTokenRequest {
    /// Saved Moyasar card token identifier (`token_...`).
    pub token_id: String,
}

/// Request payload for charging a customer's saved card token via `CustomersService`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerChargeRequest {
    /// Charge amount in minor currency units.
    pub amount: u64,
    /// Three-letter ISO 4217 currency code.
    pub currency: Currency,
}

/// Request payload for charging a recurring subscription billing cycle off-session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionChargeRequest {
    /// Merchant subscription identifier.
    pub subscription_id: String,
    /// Monotonic billing cycle index used for deterministic UUIDv5 idempotency.
    pub cycle_index: u64,
    /// Charge amount in minor currency units.
    pub amount: u64,
    /// Active saved Moyasar card token identifier (`token_...`).
    pub token_id: String,
}

/// Serializable payment summary returned by Stripe-compatible customer and subscription routes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StripePaymentResponse {
    /// Unique Moyasar payment identifier.
    pub id: String,
    /// Lifecycle status of the payment.
    pub status: PaymentStatus,
    /// Charged amount in minor currency units.
    pub amount: u64,
    /// Three-letter ISO 4217 currency code.
    pub currency: Currency,
}

impl From<Payment> for StripePaymentResponse {
    fn from(payment: Payment) -> Self {
        Self {
            id: payment.id,
            status: payment.status,
            amount: payment.amount,
            currency: payment.currency,
        }
    }
}

/// Request payload for creating a unified payment intent via `UnifiedPaymentGateway`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedIntentDto {
    /// Intent amount in minor currency units.
    pub amount_minor: u64,
    /// Three-letter ISO 4217 currency code string (for example, `"SAR"`).
    pub currency: String,
    /// HTTPS redirect callback URL invoked after checkout completion.
    pub callback_url: String,
}

/// Request payload for charging a saved card token via `UnifiedPaymentGateway`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedChargeDto {
    /// Charge amount in minor currency units.
    pub amount_minor: u64,
    /// Three-letter ISO 4217 currency code string (for example, `"SAR"`).
    pub currency: String,
    /// Active saved Moyasar card token identifier (`token_...`).
    pub token_id: String,
    /// Caller-supplied UUID idempotency key (`given_id`).
    pub idempotency_key: Uuid,
}

/// Request payload for reversing a transaction via `UnifiedPaymentGateway::refund_or_void`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedReversalDto {
    /// Target Moyasar payment or transaction identifier.
    pub transaction_id: String,
    /// Optional partial refund amount in minor units; `None` reverses the full amount.
    pub amount_minor: Option<u64>,
}

/// Creates a Stripe-compatible refund receipt or smart unsettled void (`POST /stripe/refunds`).
#[rocket::post("/stripe/refunds", data = "<req>")]
pub async fn create_stripe_refund(
    state: &State<RocketAppState>,
    req: Json<RefundCreateParams>,
) -> Result<Json<RefundReceipt>, AppError> {
    let receipt = state.client.refunds().create(req.into_inner()).await?;
    Ok(Json(receipt))
}

/// Attaches a saved card token to `customer_id` (`POST /stripe/customers/<customer_id>/tokens`).
#[rocket::post("/stripe/customers/<customer_id>/tokens", data = "<req>")]
pub async fn attach_customer_token(
    state: &State<RocketAppState>,
    customer_id: &str,
    req: Json<AttachCustomerTokenRequest>,
) -> Result<Status, AppError> {
    let token_id = TokenId::new(req.into_inner().token_id)?;
    state
        .client
        .customers(&state.customer_vault)
        .attach_token(customer_id, token_id)
        .await?;
    Ok(Status::NoContent)
}

/// Charges a customer's attached token (`POST /stripe/customers/<customer_id>/charges`).
#[rocket::post("/stripe/customers/<customer_id>/charges", data = "<req>")]
pub async fn charge_customer(
    state: &State<RocketAppState>,
    customer_id: &str,
    req: Json<CustomerChargeRequest>,
) -> Result<Json<StripePaymentResponse>, AppError> {
    let body = req.into_inner();
    let params = CustomerChargeParams::new(customer_id, body.amount, body.currency);
    let payment = state
        .client
        .customers(&state.customer_vault)
        .charge(&params)
        .await?;
    Ok(Json(StripePaymentResponse::from(payment)))
}

/// Charges a recurring subscription billing cycle (`POST /stripe/subscriptions/charge_cycle`).
#[rocket::post("/stripe/subscriptions/charge_cycle", data = "<req>")]
pub async fn charge_subscription_cycle(
    state: &State<RocketAppState>,
    req: Json<SubscriptionChargeRequest>,
) -> Result<Json<StripePaymentResponse>, AppError> {
    let body = req.into_inner();
    let token_id = TokenId::new(body.token_id)?;
    let params = SubscriptionCycleChargeParams::new(
        &body.subscription_id,
        body.cycle_index,
        body.amount,
        &token_id,
    );
    let payment = state.client.subscriptions().charge_cycle(params).await?;
    Ok(Json(StripePaymentResponse::from(payment)))
}

/// Creates a payment intent via `UnifiedPaymentGateway` (`POST /unified/intents`).
#[rocket::post("/unified/intents", data = "<req>")]
pub async fn create_unified_intent(
    state: &State<RocketAppState>,
    req: Json<UnifiedIntentDto>,
) -> Result<status::Custom<Json<UnifiedIntentResponse>>, AppError> {
    let body = req.into_inner();
    let cur = &body.currency;
    let cb = &body.callback_url;
    let intent_req = UnifiedIntentRequest::new(body.amount_minor, cur, cb);
    let intent = UnifiedPaymentGateway::create_intent(&state.client, intent_req).await?;
    Ok(status::Custom(Status::Created, Json(intent)))
}

/// Charges a saved token off-session via `UnifiedPaymentGateway` (`POST /unified/charges`).
#[rocket::post("/unified/charges", data = "<req>")]
pub async fn create_unified_charge(
    state: &State<RocketAppState>,
    req: Json<UnifiedChargeDto>,
) -> Result<Json<UnifiedChargeResponse>, AppError> {
    let body = req.into_inner();
    let charge_req = UnifiedTokenChargeRequest::new(
        body.amount_minor,
        &body.currency,
        &body.token_id,
        body.idempotency_key,
    );
    let charge = UnifiedPaymentGateway::charge_saved_token(&state.client, charge_req).await?;
    Ok(Json(charge))
}

/// Reverses a payment via refund or void (`POST /unified/reversals`).
#[rocket::post("/unified/reversals", data = "<req>")]
pub async fn create_unified_reversal(
    state: &State<RocketAppState>,
    req: Json<UnifiedReversalDto>,
) -> Result<Json<UnifiedReversalResponse>, AppError> {
    let body = req.into_inner();
    let id = &body.transaction_id;
    let rev = UnifiedPaymentGateway::refund_or_void(&state.client, id, body.amount_minor).await?;
    Ok(Json(rev))
}

/// Returns all routes for Stripe compatibility services and the unified payment gateway.
#[must_use]
pub fn routes() -> Vec<Route> {
    rocket::routes![
        create_stripe_refund,
        attach_customer_token,
        charge_customer,
        charge_subscription_cycle,
        create_unified_intent,
        create_unified_charge,
        create_unified_reversal,
    ]
}
