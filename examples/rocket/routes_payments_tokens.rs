//! Rocket routes for `PaymentsService`, `PaymentIntentsService`, `TokensService`,
//! and `CardAuthsService`.

use chrono::{DateTime, Utc};
use rocket::http::Status;
use rocket::response::status;
use rocket::serde::json::Json;
use rocket::{Route, State};
use serde::{Deserialize, Serialize};
use some_moyasar::prelude::{
    CardAuthId, CardAuthStatus, CardAuthentication, CreatePaymentIntentParams, Currency, IntentId,
    Payment, PaymentId, PaymentIntent, PaymentIntentStatus, PaymentStatus, Token, TokenId,
};

use crate::responder::MoyasarResponder as AppError;
use crate::state::RocketAppState;

/// Serializable payment summary returned by Rocket payment routes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentResponse {
    /// Unique Moyasar payment identifier (`pay_...`).
    pub id: String,
    /// Current lifecycle status of the payment.
    pub status: PaymentStatus,
    /// Payment amount in minor currency units (for example, Halalas for SAR).
    pub amount: u64,
    /// Total refunded amount in minor currency units.
    pub refunded: u64,
    /// Total captured amount in minor currency units.
    pub captured: u64,
    /// Three-letter ISO 4217 currency code.
    pub currency: Currency,
}

impl From<Payment> for PaymentResponse {
    fn from(payment: Payment) -> Self {
        Self {
            id: payment.id,
            status: payment.status,
            amount: payment.amount,
            refunded: payment.refunded,
            captured: payment.captured,
            currency: payment.currency,
        }
    }
}

/// Request payload for creating a server-locked Moyasar payment intent (`POST /payment_intents`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateIntentRequest {
    /// Locked payment amount in minor currency units (for example, Halalas for SAR).
    pub amount: u64,
    /// Locked three-letter ISO 4217 currency code.
    pub currency: Currency,
    /// Locked redirect callback URL invoked after 3DS or payment completion.
    pub callback_url: String,
}

/// Serializable payment intent summary returned by Rocket payment intent routes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentIntentResponse {
    /// Unique Moyasar payment intent identifier (`pi_...`).
    pub id: IntentId,
    /// Current lifecycle status of the payment intent.
    pub status: PaymentIntentStatus,
    /// Locked payment amount in minor currency units.
    pub amount: u64,
    /// Locked three-letter ISO 4217 currency code.
    pub currency: Currency,
    /// UTC timestamp when the payment intent expires.
    pub expires_at: DateTime<Utc>,
}

impl From<PaymentIntent> for PaymentIntentResponse {
    fn from(intent: PaymentIntent) -> Self {
        Self {
            id: intent.id,
            status: intent.status,
            amount: intent.amount,
            currency: intent.currency,
            expires_at: intent.expires_at,
        }
    }
}

/// Serializable standalone 3D Secure card authentication summary returned by Rocket routes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardAuthResponse {
    /// Validated `ca_`-prefixed standalone card authentication identifier.
    pub id: CardAuthId,
    /// Current 3DS lifecycle status (`initiated`, `authenticated`, `failed`, or `unknown`).
    pub status: CardAuthStatus,
    /// Optional hosted 3DS challenge preparation URL for cardholder redirect.
    pub transaction_url: Option<String>,
}

impl From<CardAuthentication> for CardAuthResponse {
    fn from(auth: CardAuthentication) -> Self {
        Self {
            id: auth.id,
            status: auth.status,
            transaction_url: auth.transaction_url,
        }
    }
}

/// Retrieves a payment by its `pay_`-prefixed identifier (`GET /payments/<id>`).
#[rocket::get("/payments/<id>")]
pub async fn retrieve_payment(
    state: &State<RocketAppState>,
    id: &str,
) -> Result<Json<PaymentResponse>, AppError> {
    let payment_id = PaymentId::new(id)?;
    let payment = state.client.payments().retrieve(&payment_id).await?;
    Ok(Json(PaymentResponse::from(payment)))
}

/// Voids an unsettled or authorized payment (`POST /payments/<id>/void`).
#[rocket::post("/payments/<id>/void")]
pub async fn void_payment(
    state: &State<RocketAppState>,
    id: &str,
) -> Result<Json<PaymentResponse>, AppError> {
    let payment = state.client.payments().void(id).await?;
    Ok(Json(PaymentResponse::from(payment)))
}

/// Captures the full authorized amount of a payment (`POST /payments/<id>/capture`).
#[rocket::post("/payments/<id>/capture")]
pub async fn capture_payment(
    state: &State<RocketAppState>,
    id: &str,
) -> Result<Json<PaymentResponse>, AppError> {
    let payment = state.client.payments().capture(id, None).await?;
    Ok(Json(PaymentResponse::from(payment)))
}

/// Refunds the full settled amount of a payment (`POST /payments/<id>/refund`).
#[rocket::post("/payments/<id>/refund")]
pub async fn refund_payment(
    state: &State<RocketAppState>,
    id: &str,
) -> Result<Json<PaymentResponse>, AppError> {
    let payment = state.client.payments().refund(id, None).await?;
    Ok(Json(PaymentResponse::from(payment)))
}

/// Creates a server-locked Moyasar payment intent (`POST /payment_intents`).
#[rocket::post("/payment_intents", data = "<req>")]
pub async fn create_payment_intent(
    state: &State<RocketAppState>,
    req: Json<CreateIntentRequest>,
) -> Result<status::Custom<Json<PaymentIntentResponse>>, AppError> {
    let payload = req.into_inner();
    let params =
        CreatePaymentIntentParams::new(payload.amount, payload.currency, payload.callback_url);
    let intent = state.client.payment_intents().create(params).await?;
    let body = Json(PaymentIntentResponse::from(intent));
    Ok(status::Custom(Status::Created, body))
}

/// Retrieves a payment intent by its identifier (`GET /payment_intents/<id>`).
#[rocket::get("/payment_intents/<id>")]
pub async fn retrieve_payment_intent(
    state: &State<RocketAppState>,
    id: &str,
) -> Result<Json<PaymentIntentResponse>, AppError> {
    let intent_id = IntentId::new(id)?;
    let intent = state.client.payment_intents().retrieve(&intent_id).await?;
    Ok(Json(PaymentIntentResponse::from(intent)))
}

/// Cancels an active payment intent (`POST /payment_intents/<id>/cancel`).
#[rocket::post("/payment_intents/<id>/cancel")]
pub async fn cancel_payment_intent(
    state: &State<RocketAppState>,
    id: &str,
) -> Result<Json<PaymentIntentResponse>, AppError> {
    let intent_id = IntentId::new(id)?;
    let intent = state.client.payment_intents().cancel(&intent_id).await?;
    Ok(Json(PaymentIntentResponse::from(intent)))
}

/// Retrieves a saved card token by its `token_`-prefixed identifier (`GET /tokens/<id>`).
#[rocket::get("/tokens/<id>")]
pub async fn retrieve_token(
    state: &State<RocketAppState>,
    id: &str,
) -> Result<Json<Token>, AppError> {
    let token_id = TokenId::new(id)?;
    let token = state.client.tokens().retrieve(&token_id).await?;
    Ok(Json(token))
}

/// Deletes a saved card token (`DELETE /tokens/<id>`), returning `204 No Content`.
#[rocket::delete("/tokens/<id>")]
pub async fn delete_token(state: &State<RocketAppState>, id: &str) -> Result<Status, AppError> {
    let token_id = TokenId::new(id)?;
    state.client.tokens().delete(&token_id).await?;
    Ok(Status::NoContent)
}

/// Retrieves a standalone 3DS card authentication (`GET /card_auths/<id>`).
#[rocket::get("/card_auths/<id>")]
pub async fn retrieve_card_auth(
    state: &State<RocketAppState>,
    id: &str,
) -> Result<Json<CardAuthResponse>, AppError> {
    let auth_id = CardAuthId::new(id)?;
    let auth = state.client.card_auths().retrieve(&auth_id).await?;
    Ok(Json(CardAuthResponse::from(auth)))
}

/// Returns all routes for payments, payment intents, tokens, and card authentications.
#[must_use]
pub fn routes() -> Vec<Route> {
    rocket::routes![
        retrieve_payment,
        void_payment,
        capture_payment,
        refund_payment,
        create_payment_intent,
        retrieve_payment_intent,
        cancel_payment_intent,
        retrieve_token,
        delete_token,
        retrieve_card_auth,
    ]
}
