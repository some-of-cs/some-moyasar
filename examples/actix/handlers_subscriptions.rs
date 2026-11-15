//! Actix-Web handlers for subscriptions, payouts, settlements, sources, Apple Pay, and unified gateway.

use actix_web::{HttpResponse, web};
use chrono::{DateTime, Utc};
use secrecy::SecretString;
use serde::Deserialize;
use serde_json::json;
use some_moyasar::prelude::{
    BankDestination, BinLookupParams, CreatePayoutAccountParams, CreatePayoutParams, Currency,
    InitiateApplePayParams, ListInternalTransactionsParams, ListPayoutAccountsParams,
    ListSettlementsParams, ListTransfersParams, PayoutAccountId, PayoutAccountProperties,
    PayoutAccountType, PayoutDestination, PayoutId, PayoutPurpose, RefundStrategy,
    SubscriptionCycleChargeParams, SubscriptionCycleRefundParams, TokenId, UnifiedIntentRequest,
    UnifiedPaymentGateway,
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

/// Request payload for `POST /v1/payout_accounts`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreatePayoutAccountInput {
    /// Saudi IBAN (`SA` + 22 digits).
    pub iban: String,
    /// Account holder name.
    pub name: String,
    /// Bank name.
    pub bank_name: Option<String>,
    /// Secret bank credentials payload.
    pub credentials: String,
}

/// Request payload for `POST /v1/payouts`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreatePayoutInput {
    /// Source payout account identifier.
    pub source_id: String,
    /// Payout amount in integer minor units.
    pub amount: u64,
    /// Payout purpose enum value.
    pub purpose: PayoutPurpose,
    /// Beneficiary Saudi IBAN (`SA` + 22 digits).
    pub iban: String,
    /// Beneficiary name.
    pub name: String,
    /// Beneficiary Saudi mobile number.
    pub mobile: String,
    /// Beneficiary country code.
    pub country: String,
    /// Beneficiary city.
    pub city: String,
}

/// Request payload for `POST /v1/sources/issuer`.
#[derive(Debug, Clone, Deserialize)]
pub struct BinLookupInput {
    /// 6- to 8-digit card BIN/IIN prefix.
    pub first_digits: String,
}

/// Request payload for `POST /v1/applepay/initiate`.
#[derive(Debug, Clone, Deserialize)]
pub struct InitiateApplePayInput {
    /// Apple Pay validation URL (`https://...`).
    pub validation_url: String,
    /// Merchant display name.
    pub display_name: String,
    /// Verified merchant domain name.
    pub domain_name: String,
    /// Merchant publishable API key.
    pub publishable_api_key: String,
}

/// Request payload for `POST /v1/unified/intents`.
#[derive(Debug, Clone, Deserialize)]
pub struct UnifiedIntentInput {
    /// Payment amount in integer minor units.
    pub amount_minor: u64,
    /// Three-letter ISO 4217 currency code string.
    pub currency: String,
    /// Redirect callback URL.
    pub callback_url: String,
    /// Optional payment description.
    pub description: Option<String>,
}

/// Registers subscription, payout, settlement, ledger, source, Apple Pay, and unified routes.
pub fn configure(cfg: &mut web::ServiceConfig) {
    configure_subscription_and_payout_routes(cfg);
    configure_ledger_and_source_routes(cfg);
}

fn configure_subscription_and_payout_routes(cfg: &mut web::ServiceConfig) {
    cfg.route(
        "/v1/subscriptions/{sub_id}/charge",
        web::post().to(charge_subscription_cycle),
    )
    .route(
        "/v1/subscriptions/{sub_id}/refund",
        web::post().to(refund_subscription_cycle),
    )
    .route(
        "/v1/payout_accounts",
        web::post().to(create_payout_account),
    )
    .route("/v1/payout_accounts", web::get().to(list_payout_accounts))
    .route("/v1/payouts", web::post().to(create_payout))
    .route("/v1/payouts/{id}", web::get().to(get_payout));
}

fn configure_ledger_and_source_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/v1/settlements", web::get().to(list_settlements))
        .route(
            "/v1/settlements/balance",
            web::get().to(get_merchant_balance),
        )
        .route("/v1/transfers", web::get().to(list_transfers))
        .route(
            "/v1/internal_transactions",
            web::get().to(list_internal_transactions),
        )
        .route("/v1/sources/issuer", web::post().to(lookup_bin_issuer))
        .route("/v1/applepay/initiate", web::post().to(initiate_apple_pay))
        .route(
            "/v1/unified/intents",
            web::post().to(create_unified_intent),
        );
}

/// Charges a subscription billing cycle off-session (`POST /v1/subscriptions/{sub_id}/charge`).
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

/// Handles `POST /v1/payout_accounts` via `PayoutAccountsService::create`.
pub async fn create_payout_account(
    state: web::Data<ActixAppState>,
    body: web::Json<CreatePayoutAccountInput>,
) -> Result<HttpResponse, AppError> {
    let props = PayoutAccountProperties {
        iban: Some(body.iban.clone()),
        name: Some(body.name.clone()),
        bank_name: body.bank_name.clone(),
        ..Default::default()
    };
    let params = CreatePayoutAccountParams::new(
        PayoutAccountType::Bank,
        props,
        SecretString::from(body.credentials.clone()),
    )?;
    let account = state.client.payout_accounts().create(&params).await?;
    Ok(HttpResponse::Created().json(account))
}

/// Handles `GET /v1/payout_accounts` via `PayoutAccountsService::list`.
pub async fn list_payout_accounts(
    state: web::Data<ActixAppState>,
) -> Result<HttpResponse, AppError> {
    let page = state
        .client
        .payout_accounts()
        .list(&ListPayoutAccountsParams::new())
        .await?;
    Ok(HttpResponse::Ok().json(json!({
        "items": page.items,
        "total_count": page.meta.total_count,
    })))
}

/// Handles `POST /v1/payouts` via `PayoutsService::create`.
pub async fn create_payout(
    state: web::Data<ActixAppState>,
    body: web::Json<CreatePayoutInput>,
) -> Result<HttpResponse, AppError> {
    let source_id = PayoutAccountId::new(&body.source_id)?;
    let dest = PayoutDestination::Bank(BankDestination {
        iban: body.iban.clone(),
        name: body.name.clone(),
        mobile: body.mobile.clone(),
        country: body.country.clone(),
        city: body.city.clone(),
    });
    let params = CreatePayoutParams {
        source_id,
        sequence_number: None,
        amount: body.amount,
        purpose: body.purpose,
        destination: dest,
        comment: None,
        metadata: None,
    };
    let payout = state.client.payouts().create(&params).await?;
    Ok(HttpResponse::Created().json(payout))
}

/// Handles `GET /v1/payouts/{id}` via `PayoutsService::retrieve`.
pub async fn get_payout(
    state: web::Data<ActixAppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let id = PayoutId::new(path.into_inner())?;
    let payout = state.client.payouts().retrieve(&id).await?;
    Ok(HttpResponse::Ok().json(payout))
}

/// Handles `GET /v1/settlements` via `SettlementsService::list`.
pub async fn list_settlements(state: web::Data<ActixAppState>) -> Result<HttpResponse, AppError> {
    let page = state
        .client
        .settlements()
        .list(&ListSettlementsParams::new())
        .await?;
    Ok(HttpResponse::Ok().json(json!({
        "items": page.items,
        "total_count": page.meta.total_count,
    })))
}

/// Handles `GET /v1/settlements/balance` via `SettlementsService::fetch_balance`.
pub async fn get_merchant_balance(
    state: web::Data<ActixAppState>,
) -> Result<HttpResponse, AppError> {
    let balance = state.client.settlements().fetch_balance().await?;
    Ok(HttpResponse::Ok().json(balance))
}

/// Handles `GET /v1/transfers` via `TransfersService::list`.
pub async fn list_transfers(state: web::Data<ActixAppState>) -> Result<HttpResponse, AppError> {
    let page = state
        .client
        .transfers()
        .list(&ListTransfersParams::new())
        .await?;
    Ok(HttpResponse::Ok().json(json!({
        "items": page.items,
        "total_count": page.meta.total_count,
    })))
}

/// Handles `GET /v1/internal_transactions` via `InternalTransactionsService::list`.
pub async fn list_internal_transactions(
    state: web::Data<ActixAppState>,
) -> Result<HttpResponse, AppError> {
    let page = state
        .client
        .internal_transactions()
        .list(&ListInternalTransactionsParams::new())
        .await?;
    Ok(HttpResponse::Ok().json(json!({
        "items": page.items,
        "total_count": page.meta.total_count,
    })))
}

/// Handles `POST /v1/sources/issuer` via `SourcesService::get_issuer`.
pub async fn lookup_bin_issuer(
    state: web::Data<ActixAppState>,
    body: web::Json<BinLookupInput>,
) -> Result<HttpResponse, AppError> {
    let params = BinLookupParams::new(&body.first_digits)?;
    let issuer = state.client.sources().get_issuer(&params).await?;
    Ok(HttpResponse::Ok().json(json!({
        "issuer_name": issuer.issuer_name,
        "issuer_country": issuer.issuer_country,
        "issuer_card_type": issuer.issuer_card_type,
        "first_digits": issuer.first_digits,
    })))
}

/// Handles `POST /v1/applepay/initiate` via `ApplePayService::initiate_session`.
pub async fn initiate_apple_pay(
    state: web::Data<ActixAppState>,
    body: web::Json<InitiateApplePayInput>,
) -> Result<HttpResponse, AppError> {
    let params = InitiateApplePayParams::new(
        &body.validation_url,
        &body.display_name,
        &body.domain_name,
        &body.publishable_api_key,
    )?;
    let session = state.pub_client.apple_pay().initiate_session(&params).await?;
    Ok(HttpResponse::Ok().json(session))
}

/// Handles `POST /v1/unified/intents` via `UnifiedPaymentGateway::create_intent`.
pub async fn create_unified_intent(
    state: web::Data<ActixAppState>,
    body: web::Json<UnifiedIntentInput>,
) -> Result<HttpResponse, AppError> {
    let mut req =
        UnifiedIntentRequest::new(body.amount_minor, &body.currency, &body.callback_url);
    if let Some(ref desc) = body.description {
        req = req.description(desc);
    }
    let resp = state.client.create_intent(req).await?;
    Ok(HttpResponse::Created().json(resp))
}
