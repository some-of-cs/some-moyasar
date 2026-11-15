//! Token, standalone 3DS card authentication, and hosted invoice route handlers.
#![forbid(unsafe_code)]

use core::fmt;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use some_moyasar::prelude::{
    Amount, CardAuthId, CardAuthStatus, CardAuthentication, CreateCardAuthParams,
    CreateInvoiceParams, CreateTokenParams, Currency, Invoice, Metadata, MoyasarError, Token,
    TokenId,
};
use uuid::Uuid;

use super::error::AppError;
use super::state::AppState;

/// Request payload for `POST /tokens`.
#[derive(Deserialize)]
pub struct CreateTokenRequest {
    /// Cardholder full name.
    pub name: String,
    /// Card Primary Account Number (PAN).
    pub number: String,
    /// Two-digit expiration month (`01`–`12`).
    pub month: String,
    /// Two- or four-digit expiration year.
    pub year: String,
    /// Card verification code (3 or 4 digits).
    pub cvc: String,
    /// Optional 3DS verification redirect URL.
    pub callback_url: Option<String>,
    /// Optional flag to tokenize the card without an immediate verification charge.
    pub save_only: Option<bool>,
    /// Optional validated key/value metadata.
    pub metadata: Option<Metadata>,
}

impl fmt::Debug for CreateTokenRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateTokenRequest")
            .field("name", &self.name)
            .field("number", &"[REDACTED]")
            .field("month", &self.month)
            .field("year", &self.year)
            .field("cvc", &"[REDACTED]")
            .field("callback_url", &self.callback_url)
            .field("save_only", &self.save_only)
            .field("metadata", &self.metadata)
            .finish()
    }
}

/// Request payload for `POST /card_auths`.
#[derive(Deserialize)]
pub struct CreateCardAuthRequest {
    /// Authentication amount in integer minor currency units.
    pub amount: Amount,
    /// ISO 4217 currency code.
    pub currency: Currency,
    /// HTTPS callback URL where the cardholder returns after 3DS challenge.
    pub callback_url: String,
    /// Cardholder full name.
    pub name: String,
    /// Card Primary Account Number (PAN).
    pub number: String,
    /// Card verification code (3 or 4 digits).
    pub cvc: String,
    /// Expiration month (`1`–`12`).
    pub month: u8,
    /// Four-digit expiration year (e.g., `2028`).
    pub year: u16,
    /// Optional human-readable description.
    pub description: Option<String>,
}

impl fmt::Debug for CreateCardAuthRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateCardAuthRequest")
            .field("amount", &self.amount)
            .field("currency", &self.currency)
            .field("callback_url", &self.callback_url)
            .field("name", &self.name)
            .field("number", &"[REDACTED]")
            .field("cvc", &"[REDACTED]")
            .field("month", &self.month)
            .field("year", &self.year)
            .field("description", &self.description)
            .finish()
    }
}

/// Redacted JSON response DTO for [`CardAuthentication`].
#[derive(Debug, Serialize)]
pub struct CardAuthResponse {
    /// Unique Moyasar 3DS card authentication identifier (`ca_...`).
    pub id: CardAuthId,
    /// Current 3DS lifecycle status (`initiated`, `authenticated`, `failed`, `unknown`).
    pub status: CardAuthStatus,
    /// Optional 3DS ACS challenge redirect URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_url: Option<String>,
    /// Whether the 3DS authentication completed via a frictionless flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_frictionless: Option<bool>,
}

impl From<CardAuthentication> for CardAuthResponse {
    fn from(auth: CardAuthentication) -> Self {
        let is_frictionless = auth.result.as_ref().and_then(|res| res.is_frictionless);
        Self {
            id: auth.id,
            status: auth.status,
            transaction_url: auth.transaction_url,
            is_frictionless,
        }
    }
}

/// Request payload for `POST /invoices`.
#[derive(Debug, Deserialize)]
pub struct CreateInvoiceRequest {
    /// Invoice amount in integer minor currency units.
    pub amount: Amount,
    /// ISO 4217 currency code.
    pub currency: Currency,
    /// Optional human-readable invoice description.
    pub description: Option<String>,
    /// Optional webhook/redirect callback URL.
    pub callback_url: Option<String>,
    /// Optional redirect URL on successful payment.
    pub success_url: Option<String>,
    /// Optional merchant return URL (`back_url`).
    pub back_url: Option<String>,
    /// Optional expiration timestamp in UTC.
    pub expired_at: Option<DateTime<Utc>>,
    /// Optional validated key/value metadata.
    pub metadata: Option<Metadata>,
}

impl From<CreateInvoiceRequest> for CreateInvoiceParams {
    fn from(req: CreateInvoiceRequest) -> Self {
        Self {
            amount: req.amount,
            currency: req.currency,
            description: req.description,
            callback_url: req.callback_url,
            success_url: req.success_url,
            back_url: req.back_url,
            expired_at: req.expired_at,
            metadata: req.metadata,
        }
    }
}

/// Builds the Axum [`Router`] for tokens, standalone 3DS card authentications, and invoices.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tokens", post(create_token))
        .route("/tokens/{id}", get(retrieve_token).delete(delete_token))
        .route("/card_auths", post(create_card_auth))
        .route("/card_auths/{id}", get(retrieve_card_auth))
        .route("/invoices", post(create_invoice))
        .route("/invoices/{id}", get(retrieve_invoice))
        .route("/invoices/{id}/cancel", put(cancel_invoice))
}

fn build_token_params(req: CreateTokenRequest) -> CreateTokenParams {
    let mut params = CreateTokenParams::new(req.name, req.number, req.month, req.year, req.cvc);
    if let Some(callback_url) = req.callback_url {
        params = params.callback_url(callback_url);
    }
    if let Some(save_only) = req.save_only {
        params = params.save_only(save_only);
    }
    if let Some(metadata) = req.metadata {
        params = params.metadata(metadata);
    }
    params
}

fn build_card_auth_params(
    req: CreateCardAuthRequest,
) -> Result<CreateCardAuthParams, MoyasarError> {
    let mut params = CreateCardAuthParams::new(
        req.amount,
        req.currency,
        req.callback_url,
        req.name,
        SecretString::from(req.number),
        SecretString::from(req.cvc),
        req.month,
        req.year,
    )?;
    if let Some(description) = req.description {
        params = params.with_description(description);
    }
    Ok(params)
}

/// Handles `POST /tokens` by tokenizing a card via `TokensService::create` (`201 Created`).
async fn create_token(
    State(state): State<AppState>,
    Json(payload): Json<CreateTokenRequest>,
) -> Result<(StatusCode, Json<Token>), AppError> {
    let params = build_token_params(payload);
    let token = state.client.tokens().create(&params).await?;
    Ok((StatusCode::CREATED, Json(token)))
}

/// Handles `GET /tokens/{id}` by retrieving a stored token via `TokensService::retrieve` (`200 OK`).
async fn retrieve_token(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Token>, AppError> {
    let token_id = TokenId::new(id)?;
    let token = state.client.tokens().retrieve(&token_id).await?;
    Ok(Json(token))
}

/// Handles `DELETE /tokens/{id}` by deleting a stored token via `TokensService::delete` (`204 No Content`).
async fn delete_token(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let token_id = TokenId::new(id)?;
    state.client.tokens().delete(&token_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Handles `POST /card_auths` by initiating a standalone 3DS authentication (`201 Created`).
async fn create_card_auth(
    State(state): State<AppState>,
    Json(payload): Json<CreateCardAuthRequest>,
) -> Result<(StatusCode, Json<CardAuthResponse>), AppError> {
    let params = build_card_auth_params(payload)?;
    let auth = state.client.card_auths().create(&params).await?;
    Ok((StatusCode::CREATED, Json(CardAuthResponse::from(auth))))
}

/// Handles `GET /card_auths/{id}` by retrieving a standalone 3DS authentication (`200 OK`).
async fn retrieve_card_auth(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<CardAuthResponse>, AppError> {
    let auth_id = CardAuthId::new(id)?;
    let auth = state.client.card_auths().retrieve(&auth_id).await?;
    Ok(Json(CardAuthResponse::from(auth)))
}

/// Handles `POST /invoices` by creating a hosted e-invoice (`201 Created`).
async fn create_invoice(
    State(state): State<AppState>,
    Json(payload): Json<CreateInvoiceRequest>,
) -> Result<(StatusCode, Json<Invoice>), AppError> {
    let params = CreateInvoiceParams::from(payload);
    let invoice = state.client.invoices().create(&params).await?;
    Ok((StatusCode::CREATED, Json(invoice)))
}

/// Handles `GET /invoices/{id}` by retrieving a hosted e-invoice by UUID (`200 OK`).
async fn retrieve_invoice(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Invoice>, AppError> {
    let invoice = state.client.invoices().retrieve(id).await?;
    Ok(Json(invoice))
}

/// Handles `PUT /invoices/{id}/cancel` by canceling an active hosted e-invoice (`200 OK`).
async fn cancel_invoice(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Invoice>, AppError> {
    let invoice = state.client.invoices().cancel(id).await?;
    Ok(Json(invoice))
}
