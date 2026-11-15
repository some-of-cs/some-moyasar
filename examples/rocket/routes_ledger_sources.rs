//! Rocket routes for `TransfersService`, `InternalTransactionsService`,
//! `SourcesService`, and `ApplePayService`.

use rocket::serde::json::Json;
use rocket::{Route, State};
use serde::{Deserialize, Serialize};
use some_moyasar::prelude::{
    ApplePayMerchantSession, BinLookupParams, InitiateApplePayParams, InternalTransaction,
    ListInternalTransactionsParams, ListTransfersParams, MoyasarError, SourceIssuerCardType,
    Transfer, TransferId,
};

use crate::responder::MoyasarResponder;
use crate::state::RocketAppState;

/// Rocket error responder alias for ledger, sources, and Apple Pay routes.
pub type AppError = MoyasarResponder;

/// Serializable response payload for the paginated aggregation transfers listing endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransfersListResponse {
    /// Aggregation transfer records returned on the current page.
    pub items: Vec<Transfer>,
    /// Total number of matching transfer records across all pages.
    pub total_count: Option<u64>,
    /// Current 1-based page number.
    pub current_page: Option<u32>,
    /// Next page number, if more results exist.
    pub next_page: Option<u32>,
}

/// Serializable response payload for the paginated internal transactions listing endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InternalTransactionsListResponse {
    /// Internal ledger transaction records returned on the current page.
    pub items: Vec<InternalTransaction>,
    /// Total number of matching internal transaction records across all pages.
    pub total_count: Option<u64>,
    /// Current 1-based page number.
    pub current_page: Option<u32>,
    /// Next page number, if more results exist.
    pub next_page: Option<u32>,
}

/// Request payload for looking up card issuer metadata by 6-to-8-digit BIN/IIN prefix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssuerLookupRequest {
    /// Leading 6- to 8-digit card BIN/IIN prefix.
    pub first_digits: String,
}

/// Serializable response payload for card BIN/IIN issuer metadata lookup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssuerResponse {
    /// Issuing bank or financial institution name, if available.
    pub issuer_name: Option<String>,
    /// Two-letter ISO 3166-1 alpha-2 country code of the issuer (e.g., `"SA"`).
    pub issuer_country: Option<String>,
    /// Funding classification of the card (`debit`, `credit`, `charge_card`, etc.).
    pub issuer_card_type: SourceIssuerCardType,
    /// Leading BIN/IIN digits echoed by the Moyasar API.
    pub first_digits: String,
}

/// Request payload for initiating a signed Apple Pay Web Merchant session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplePayInitiateRequest {
    /// Apple Pay validation URL from `PKPaymentAuthorizationController` (`https://` required).
    pub validation_url: String,
    /// Merchant display name shown on the Apple Pay payment sheet.
    pub display_name: String,
    /// Verified merchant domain name initiating the session.
    pub domain_name: String,
    /// Publishable Moyasar API key (`pk_live_...` or `pk_test_...`).
    pub publishable_api_key: String,
}

/// Lists Moyasar aggregation transfers using default pagination parameters.
///
/// Maps `GET /transfers` to `TransfersService::list`.
#[rocket::get("/transfers")]
pub async fn list_transfers(
    state: &State<RocketAppState>,
) -> Result<Json<TransfersListResponse>, AppError> {
    let page = state
        .client
        .transfers()
        .list(&ListTransfersParams::new())
        .await?;
    Ok(Json(TransfersListResponse {
        items: page.items,
        total_count: Some(page.meta.total_count),
        current_page: Some(page.meta.current_page),
        next_page: page.meta.next_page,
    }))
}

/// Retrieves a single Moyasar aggregation transfer by its identifier.
///
/// Maps `GET /transfers/<id>` to `TransfersService::retrieve`.
#[rocket::get("/transfers/<id>")]
pub async fn retrieve_transfer(
    state: &State<RocketAppState>,
    id: &str,
) -> Result<Json<Transfer>, AppError> {
    let transfer = state
        .client
        .transfers()
        .retrieve(&TransferId::new(id))
        .await?;
    Ok(Json(transfer))
}

/// Lists Moyasar internal ledger transactions using default pagination parameters.
///
/// Maps `GET /internal_transactions` to `InternalTransactionsService::list`.
#[rocket::get("/internal_transactions")]
pub async fn list_internal_transactions(
    state: &State<RocketAppState>,
) -> Result<Json<InternalTransactionsListResponse>, AppError> {
    let page = state
        .client
        .internal_transactions()
        .list(&ListInternalTransactionsParams::new())
        .await?;
    Ok(Json(InternalTransactionsListResponse {
        items: page.items,
        total_count: Some(page.meta.total_count),
        current_page: Some(page.meta.current_page),
        next_page: page.meta.next_page,
    }))
}

/// Looks up card issuer metadata by 6-to-8-digit BIN/IIN prefix.
///
/// Maps `POST /sources/issuer` to `SourcesService::get_issuer`.
#[rocket::post("/sources/issuer", data = "<req>")]
pub async fn lookup_issuer(
    state: &State<RocketAppState>,
    req: Json<IssuerLookupRequest>,
) -> Result<Json<IssuerResponse>, AppError> {
    let params = BinLookupParams::new(req.into_inner().first_digits)?;
    let issuer = state.client.sources().get_issuer(&params).await?;
    Ok(Json(IssuerResponse {
        issuer_name: issuer.issuer_name,
        issuer_country: issuer.issuer_country,
        issuer_card_type: issuer.issuer_card_type,
        first_digits: issuer.first_digits,
    }))
}

/// Initiates a signed Apple Pay Web Merchant session using the publishable key client.
///
/// Maps `POST /apple_pay/session` to `ApplePayService::initiate_session`.
#[rocket::post("/apple_pay/session", data = "<req>")]
pub async fn initiate_apple_pay_session(
    state: &State<RocketAppState>,
    req: Json<ApplePayInitiateRequest>,
) -> Result<Json<ApplePayMerchantSession>, AppError> {
    let body = req.into_inner();
    let params = InitiateApplePayParams::new(
        body.validation_url,
        body.display_name,
        body.domain_name,
        body.publishable_api_key,
    )
    .map_err(|err| MoyasarError::invalid_request(err.to_string()))?;
    let session = state
        .publishable_client
        .apple_pay()
        .initiate_session(&params)
        .await?;
    Ok(Json(session))
}

/// Returns all routes for transfers, internal transactions, BIN issuer lookup, and Apple Pay.
#[must_use]
pub fn routes() -> Vec<Route> {
    rocket::routes![
        list_transfers,
        retrieve_transfer,
        list_internal_transactions,
        lookup_issuer,
        initiate_apple_pay_session,
    ]
}
