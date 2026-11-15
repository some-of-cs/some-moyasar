//! Payment, refund, customer vault, token, intent, invoice, and 3DS handlers for Actix-Web.

use actix_web::{HttpResponse, web};
use secrecy::SecretString;
use serde::Deserialize;
use serde_json::{Value, json};
use some_moyasar::prelude::{
    Amount, CardAuthId, CardAuthentication, CreateCardAuthParams, CreateInvoiceParams,
    CreatePaymentIntentParams, CreatePaymentParams, CreateTokenParams, Currency,
    CustomerChargeParams, FulfillIntentParams, FulfillPaymentSourceRequest, IntentId, ListParams,
    MoyasarError, PaymentId, PaymentIntent, PaymentSourceRequest, RefundCreateParams, TokenId,
    TokenSource, inject_customer_metadata,
};
use uuid::Uuid;

use super::error::AppError;
use super::state::ActixAppState;

/// Request payload for `POST /v1/payments`.
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

/// Request payload for `POST /v1/payments/{id}/refund`.
#[derive(Debug, Default, Deserialize)]
pub struct RefundPaymentInput {
    /// Optional partial refund amount in minor units; `None` reverses the full amount.
    pub amount: Option<u64>,
}

/// Request payload for `POST /v1/customers/{customer_id}/tokens`.
#[derive(Debug, Deserialize)]
pub struct AttachTokenInput {
    /// Saved card token identifier (`token_...`) to bind to the customer.
    pub token_id: String,
}

/// Request payload for `POST /v1/customers/{customer_id}/charge`.
#[derive(Debug, Deserialize)]
pub struct CustomerChargeInput {
    /// Charge amount in minor units.
    pub amount: u64,
    /// ISO 4217 currency code.
    pub currency: Currency,
}

/// Request payload for `POST /v1/tokens`.
#[derive(Debug, Deserialize)]
pub struct CreateTokenInput {
    /// Cardholder name.
    pub name: String,
    /// Card primary account number (held transiently and zeroized by the SDK).
    pub number: String,
    /// Two-digit expiration month.
    pub month: String,
    /// Two- or four-digit expiration year.
    pub year: String,
    /// Card verification code.
    pub cvc: String,
    /// Optional callback URL for 3DS verification.
    pub callback_url: Option<String>,
}

/// Request payload for `POST /v1/payment_intents`.
#[derive(Debug, Deserialize)]
pub struct CreateIntentInput {
    /// Locked payment amount in integer minor units.
    pub amount: u64,
    /// ISO 4217 currency code.
    pub currency: Currency,
    /// Redirect callback URL after completion.
    pub callback_url: String,
    /// Optional payment intent description.
    pub description: Option<String>,
}

/// Request payload for `POST /v1/payment_intents/{id}/fulfill`.
#[derive(Debug, Deserialize)]
pub struct FulfillIntentInput {
    /// Ephemeral client secret from the payment intent.
    pub client_secret: String,
    /// Saved card token identifier (`token_...`) used to fulfill the intent.
    pub token_id: String,
}

/// Request payload for `POST /v1/invoices`.
#[derive(Debug, Deserialize)]
pub struct CreateInvoiceInput {
    /// Invoice amount in integer minor units.
    pub amount: u64,
    /// ISO 4217 currency code.
    pub currency: Currency,
    /// Optional invoice description.
    pub description: Option<String>,
}

/// Request payload for `POST /v1/card_auths`.
#[derive(Debug, Deserialize)]
pub struct CreateCardAuthInput {
    /// Authentication amount in integer minor units.
    pub amount: u64,
    /// ISO 4217 currency code.
    pub currency: Currency,
    /// HTTPS callback URL for 3DS completion.
    pub callback_url: String,
    /// Cardholder name.
    pub name: String,
    /// Card number.
    pub number: String,
    /// Card verification code.
    pub cvc: String,
    /// Expiration month (`1..=12`).
    pub month: u8,
    /// Four-digit expiration year.
    pub year: u16,
}

/// Registers payment, refund, customer, token, intent, invoice, and 3DS routes on `cfg`.
pub fn configure(cfg: &mut web::ServiceConfig) {
    configure_payment_and_customer_routes(cfg);
    configure_token_intent_invoice_and_auth_routes(cfg);
}

fn configure_payment_and_customer_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/v1/payments", web::post().to(create_payment))
        .route("/v1/payments", web::get().to(list_payments))
        .route("/v1/payments/{id}", web::get().to(get_payment))
        .route("/v1/payments/{id}/refund", web::post().to(refund_payment))
        .route(
            "/v1/customers/{customer_id}/tokens",
            web::post().to(attach_customer_token),
        )
        .route(
            "/v1/customers/{customer_id}/charge",
            web::post().to(charge_customer),
        );
}

fn configure_token_intent_invoice_and_auth_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/v1/tokens", web::post().to(create_token))
        .route("/v1/tokens/{id}", web::get().to(get_token))
        .route("/v1/tokens/{id}", web::delete().to(delete_token))
        .route("/v1/payment_intents", web::post().to(create_payment_intent))
        .route(
            "/v1/payment_intents/{id}",
            web::get().to(get_payment_intent),
        )
        .route(
            "/v1/payment_intents/{id}/cancel",
            web::post().to(cancel_payment_intent),
        )
        .route(
            "/v1/payment_intents/{id}/fulfill",
            web::post().to(fulfill_payment_intent),
        )
        .route("/v1/invoices", web::post().to(create_invoice))
        .route("/v1/invoices/{id}", web::get().to(get_invoice))
        .route("/v1/invoices/{id}/cancel", web::put().to(cancel_invoice))
        .route("/v1/card_auths", web::post().to(create_card_auth))
        .route("/v1/card_auths/{id}", web::get().to(get_card_auth));
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

fn intent_to_json(intent: &PaymentIntent) -> Value {
    json!({
        "id": intent.id,
        "status": intent.status,
        "amount": intent.amount,
        "currency": intent.currency,
        "description": intent.description,
        "callback_url": intent.callback_url,
        "expires_at": intent.expires_at,
        "created_at": intent.created_at,
    })
}

fn card_auth_to_json(auth: &CardAuthentication) -> Value {
    json!({
        "id": auth.id,
        "status": auth.status,
        "transaction_url": auth.transaction_url,
    })
}

/// Handles `POST /v1/payments` by creating a token-backed payment via `PaymentsService`.
pub async fn create_payment(
    state: web::Data<ActixAppState>,
    body: web::Json<CreatePaymentInput>,
) -> Result<HttpResponse, AppError> {
    let req = build_payment_params(&body)?;
    let payment = state.client.payments().create(&req).await?;
    Ok(HttpResponse::Created().json(payment))
}

/// Handles `GET /v1/payments` by listing payments via `PaymentsService::list`.
pub async fn list_payments(state: web::Data<ActixAppState>) -> Result<HttpResponse, AppError> {
    let page = state.client.payments().list(&ListParams::new()).await?;
    Ok(HttpResponse::Ok().json(json!({
        "items": page.items,
        "total_count": page.meta.total_count,
    })))
}

/// Handles `GET /v1/payments/{id}` by retrieving the payment from `PaymentsService`.
pub async fn get_payment(
    state: web::Data<ActixAppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let id = PaymentId::new(path.into_inner())?;
    let payment = state.client.payments().retrieve(&id).await?;
    Ok(HttpResponse::Ok().json(payment))
}

/// Handles `POST /v1/payments/{id}/refund` via `RefundsService::create`.
pub async fn refund_payment(
    state: web::Data<ActixAppState>,
    path: web::Path<String>,
    body: Option<web::Json<RefundPaymentInput>>,
) -> Result<HttpResponse, AppError> {
    let id = PaymentId::new(path.into_inner())?;
    let mut params = RefundCreateParams::new(id.as_str()).prefer_void_if_unsettled(true);
    if let Some(amount) = body.and_then(|b| b.amount) {
        params = params.amount(amount);
    }
    let receipt = state.client.refunds().create(params).await?;
    Ok(HttpResponse::Ok().json(receipt))
}

/// Handles `POST /v1/customers/{customer_id}/tokens` via `CustomersService::attach_token`.
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

/// Handles `POST /v1/customers/{customer_id}/charge` via `CustomersService::charge`.
pub async fn charge_customer(
    state: web::Data<ActixAppState>,
    path: web::Path<String>,
    body: web::Json<CustomerChargeInput>,
) -> Result<HttpResponse, AppError> {
    let customer_id = path.into_inner();
    let params = CustomerChargeParams::new(&customer_id, body.amount, body.currency);
    let payment = state
        .client
        .customers(state.vault.as_ref())
        .charge(&params)
        .await?;
    Ok(HttpResponse::Created().json(payment))
}

/// Handles `POST /v1/tokens` via `TokensService::create` using the publishable client.
pub async fn create_token(
    state: web::Data<ActixAppState>,
    body: web::Json<CreateTokenInput>,
) -> Result<HttpResponse, AppError> {
    let mut params = CreateTokenParams::new(
        &body.name,
        body.number.clone(),
        &body.month,
        &body.year,
        body.cvc.clone(),
    )
    .save_only(true);
    if let Some(ref url) = body.callback_url {
        params = params.callback_url(url);
    }
    let token = state.pub_client.tokens().create(&params).await?;
    Ok(HttpResponse::Created().json(token))
}

/// Handles `GET /v1/tokens/{id}` via `TokensService::retrieve`.
pub async fn get_token(
    state: web::Data<ActixAppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let id = TokenId::new(path.into_inner())?;
    let token = state.client.tokens().retrieve(&id).await?;
    Ok(HttpResponse::Ok().json(token))
}

/// Handles `DELETE /v1/tokens/{id}` via `TokensService::delete`.
pub async fn delete_token(
    state: web::Data<ActixAppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let id = TokenId::new(path.into_inner())?;
    state.client.tokens().delete(&id).await?;
    Ok(HttpResponse::NoContent().finish())
}

/// Handles `POST /v1/payment_intents` via `PaymentIntentsService::create`.
pub async fn create_payment_intent(
    state: web::Data<ActixAppState>,
    body: web::Json<CreateIntentInput>,
) -> Result<HttpResponse, AppError> {
    let mut params =
        CreatePaymentIntentParams::new(body.amount, body.currency, &body.callback_url);
    if let Some(ref desc) = body.description {
        params = params.description(desc);
    }
    let intent = state.client.payment_intents().create(params).await?;
    Ok(HttpResponse::Created().json(intent_to_json(&intent)))
}

/// Handles `GET /v1/payment_intents/{id}` via `PaymentIntentsService::retrieve`.
pub async fn get_payment_intent(
    state: web::Data<ActixAppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let id = IntentId::new(path.into_inner())?;
    let intent = state.client.payment_intents().retrieve(&id).await?;
    Ok(HttpResponse::Ok().json(intent_to_json(&intent)))
}

/// Handles `POST /v1/payment_intents/{id}/cancel` via `PaymentIntentsService::cancel`.
pub async fn cancel_payment_intent(
    state: web::Data<ActixAppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let id = IntentId::new(path.into_inner())?;
    let intent = state.client.payment_intents().cancel(&id).await?;
    Ok(HttpResponse::Ok().json(intent_to_json(&intent)))
}

/// Handles `POST /v1/payment_intents/{id}/fulfill` via `PaymentIntentsService::fulfill`.
pub async fn fulfill_payment_intent(
    state: web::Data<ActixAppState>,
    path: web::Path<String>,
    body: web::Json<FulfillIntentInput>,
) -> Result<HttpResponse, AppError> {
    let id = IntentId::new(path.into_inner())?;
    let source = FulfillPaymentSourceRequest::Token {
        token: body.token_id.clone(),
    };
    let params = FulfillIntentParams::new(&body.client_secret, source)?;
    let fulfilled = state.pub_client.payment_intents().fulfill(&id, params).await?;
    Ok(HttpResponse::Ok().json(json!({
        "id": fulfilled.id,
        "status": fulfilled.status,
        "amount": fulfilled.amount,
        "currency": fulfilled.currency,
    })))
}

/// Handles `POST /v1/invoices` via `InvoicesService::create`.
pub async fn create_invoice(
    state: web::Data<ActixAppState>,
    body: web::Json<CreateInvoiceInput>,
) -> Result<HttpResponse, AppError> {
    let mut params = CreateInvoiceParams::new(Amount::from_minor(body.amount), body.currency);
    if let Some(ref desc) = body.description {
        params = params.description(desc);
    }
    let invoice = state.client.invoices().create(&params).await?;
    Ok(HttpResponse::Created().json(invoice))
}

/// Handles `GET /v1/invoices/{id}` via `InvoicesService::retrieve`.
pub async fn get_invoice(
    state: web::Data<ActixAppState>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let invoice = state.client.invoices().retrieve(path.into_inner()).await?;
    Ok(HttpResponse::Ok().json(invoice))
}

/// Handles `PUT /v1/invoices/{id}/cancel` via `InvoicesService::cancel`.
pub async fn cancel_invoice(
    state: web::Data<ActixAppState>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let invoice = state.client.invoices().cancel(path.into_inner()).await?;
    Ok(HttpResponse::Ok().json(invoice))
}

/// Handles `POST /v1/card_auths` via `CardAuthsService::create`.
pub async fn create_card_auth(
    state: web::Data<ActixAppState>,
    body: web::Json<CreateCardAuthInput>,
) -> Result<HttpResponse, AppError> {
    let params = CreateCardAuthParams::new(
        Amount::from_minor(body.amount),
        body.currency,
        &body.callback_url,
        &body.name,
        SecretString::from(body.number.clone()),
        SecretString::from(body.cvc.clone()),
        body.month,
        body.year,
    )?;
    let auth = state.client.card_auths().create(&params).await?;
    Ok(HttpResponse::Created().json(card_auth_to_json(&auth)))
}

/// Handles `GET /v1/card_auths/{id}` via `CardAuthsService::retrieve`.
pub async fn get_card_auth(
    state: web::Data<ActixAppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let id = CardAuthId::new(path.into_inner())?;
    let auth = state.client.card_auths().retrieve(&id).await?;
    Ok(HttpResponse::Ok().json(card_auth_to_json(&auth)))
}
