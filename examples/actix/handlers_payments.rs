//! Payment and customer vault HTTP handlers for Actix-Web.

use actix_web::{HttpResponse, web};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use some_moyasar::prelude::{
    CreatePaymentParams, Currency, MoyasarError, PaymentId, PaymentSourceRequest, TokenId,
    TokenSource, inject_customer_metadata,
};

use super::error::AppError;
use super::state::ActixAppState;

/// Request payload for `POST /v1/payments`.
///
/// Uses integer minor-unit `amount` (`u64`) and a validated saved card
/// `token_id` (`token_...`), with optional customer metadata injection and
/// idempotency support.
#[derive(Debug, Deserialize)]
pub struct CreatePaymentInput {
    /// Payment amount in the currency's smallest minor unit (e.g., Halalas for SAR).
    pub amount: u64,
    /// ISO 4217 currency code.
    pub currency: Currency,
    /// Saved card token identifier (`token_...`).
    pub token_id: String,
    /// Optional merchant customer identifier injected into payment metadata.
    pub customer_id: Option<String>,
    /// Optional client-supplied UUIDv4/v5 idempotency key (`given_id`).
    pub given_id: Option<Uuid>,
    /// Optional human-readable payment description.
    pub description: Option<String>,
}

/// Request payload for `POST /v1/customers/{customer_id}/tokens`.
#[derive(Debug, Deserialize)]
pub struct AttachTokenInput {
    /// Saved card token identifier (`token_...`) to bind to the customer.
    pub token_id: String,
}

fn build_payment_params(body: &CreatePaymentInput) -> Result<CreatePaymentParams, MoyasarError> {
    let source = TokenSource::new(&body.token_id)?;
    let mut builder = CreatePaymentParams::builder()
        .amount(body.amount)
        .currency(body.currency)
        .payment_source(PaymentSourceRequest::Token(source));

    if let Some(given_id) = body.given_id {
        builder = builder.given_id(given_id);
    }
    if let Some(description) = body.description.as_deref() {
        builder = builder.description(description);
    }
    if let Some(cid) = body.customer_id.as_deref() {
        builder = builder.metadata(inject_customer_metadata(None, cid)?);
    }

    builder.build()
}

/// Handles `POST /v1/payments` by creating a token-backed payment via `PaymentsService`.
///
/// Injects `metadata["customer_id"]` when `customer_id` is supplied and returns
/// `201 Created` with the created [`some_moyasar::prelude::Payment`] JSON payload.
pub async fn create_payment(
    state: web::Data<ActixAppState>,
    body: web::Json<CreatePaymentInput>,
) -> Result<HttpResponse, AppError> {
    let req = build_payment_params(&body)?;
    let payment = state.client.payments().create(&req).await?;
    Ok(HttpResponse::Created().json(payment))
}

/// Handles `GET /v1/payments/{id}` by validating the `pay_...` identifier and
/// retrieving the payment from `PaymentsService`.
///
/// Returns `200 OK` with the retrieved [`some_moyasar::prelude::Payment`] JSON payload.
pub async fn get_payment(
    state: web::Data<ActixAppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let id = PaymentId::new(&path.into_inner())?;
    let payment = state.client.payments().retrieve(&id).await?;
    Ok(HttpResponse::Ok().json(payment))
}

/// Handles `POST /v1/customers/{customer_id}/tokens` by validating `token_id`
/// and associating it with `customer_id` in the configured `CustomerVault`.
///
/// Returns `201 Created` with `{"customer_id": ..., "token_id": ...}`.
pub async fn attach_customer_token(
    state: web::Data<ActixAppState>,
    path: web::Path<String>,
    body: web::Json<AttachTokenInput>,
) -> Result<HttpResponse, AppError> {
    let customer_id = path.into_inner();
    let token_id = TokenId::new(&body.token_id)?;
    state
        .client
        .customers(state.vault.as_ref())
        .attach_token(&customer_id, token_id)
        .await?;
    Ok(HttpResponse::Created().json(json!({
        "customer_id": customer_id,
        "token_id": body.token_id,
    })))
}
