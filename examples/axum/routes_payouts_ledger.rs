//! Payout accounts, payouts, settlements, transfers, and internal transactions route handlers.
#![forbid(unsafe_code)]

use core::fmt;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use some_moyasar::kernel::{Meta, PaginatedList};
use some_moyasar::resources::payouts::accounts_create::CreatePayoutAccountParams;
use some_moyasar::resources::payouts::accounts_list::ListPayoutAccountsParams;
use some_moyasar::resources::payouts::models::{
    PayoutAccount, PayoutAccountProperties, PayoutAccountType,
};
use some_moyasar::resources::payouts::payout_model::{Payout, PayoutId};
use some_moyasar::resources::payouts::payouts_create::CreatePayoutParams;
use some_moyasar::resources::settlements::balance::MerchantBalance;
use some_moyasar::resources::settlements::internal_transactions::ListInternalTransactionsParams;
use some_moyasar::resources::settlements::internal_txn_models::InternalTransaction;
use some_moyasar::resources::settlements::models::{Settlement, SettlementId};
use some_moyasar::resources::settlements::transfer_models::{Transfer, TransferId};
use some_moyasar::resources::settlements::transfers::ListTransfersParams;

use super::error::AppError;
use super::state::AppState;

/// Request payload for `POST /payout_accounts`.
#[derive(Deserialize)]
pub struct CreatePayoutAccountRequest {
    /// Payout account destination type (`bank` or `wallet`).
    pub account_type: PayoutAccountType,
    /// Account-type-specific properties (IBAN, beneficiary name, bank name).
    #[serde(default)]
    pub properties: PayoutAccountProperties,
    /// Sensitive provider credentials (string or JSON object).
    pub credentials: Value,
}

impl fmt::Debug for CreatePayoutAccountRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreatePayoutAccountRequest")
            .field("account_type", &self.account_type)
            .field("properties", &self.properties)
            .field("credentials", &"[REDACTED]")
            .finish()
    }
}

/// Serializable pagination metadata DTO mirroring [`Meta`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PaginationMetaResponse {
    /// Current 1-indexed page number.
    pub current_page: u32,
    /// Next page number, if more pages remain.
    pub next_page: Option<u32>,
    /// Previous page number, if not on the first page.
    pub prev_page: Option<u32>,
    /// Total number of pages available.
    pub total_pages: u32,
    /// Total number of records across all pages.
    pub total_count: u64,
}

impl From<Meta> for PaginationMetaResponse {
    fn from(meta: Meta) -> Self {
        Self {
            current_page: meta.current_page,
            next_page: meta.next_page,
            prev_page: meta.prev_page,
            total_pages: meta.total_pages,
            total_count: meta.total_count,
        }
    }
}

/// Serializable paginated list envelope DTO mirroring [`PaginatedList`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PaginatedResponse<T> {
    /// Page items returned by the Moyasar API.
    pub items: Vec<T>,
    /// Pagination metadata for the current page.
    pub meta: PaginationMetaResponse,
}

impl<T> From<PaginatedList<T>> for PaginatedResponse<T> {
    fn from(list: PaginatedList<T>) -> Self {
        Self {
            items: list.items,
            meta: PaginationMetaResponse::from(list.meta),
        }
    }
}

/// Builds the Axum [`Router`] for payout accounts, payouts, settlements,
/// transfers, and internal transactions services.
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/payout_accounts",
            post(create_payout_account).get(list_payout_accounts),
        )
        .route("/payouts", post(create_payout))
        .route("/payouts/{id}", get(retrieve_payout))
        .route("/settlements/balance", get(fetch_settlement_balance))
        .route("/settlements/{id}", get(retrieve_settlement))
        .route("/transfers", get(list_transfers))
        .route("/transfers/{id}", get(retrieve_transfer))
        .route("/internal_transactions", get(list_internal_transactions))
}

/// Converts a raw JSON credential value into a redacted [`SecretString`].
fn into_secret_credentials(value: Value) -> SecretString {
    match value {
        Value::String(raw) => SecretString::from(raw),
        other => SecretString::from(other.to_string()),
    }
}

/// Handles `POST /payout_accounts` by registering a new payout source account.
async fn create_payout_account(
    State(state): State<AppState>,
    Json(payload): Json<CreatePayoutAccountRequest>,
) -> Result<(StatusCode, Json<PayoutAccount>), AppError> {
    let credentials = into_secret_credentials(payload.credentials);
    let params =
        CreatePayoutAccountParams::new(payload.account_type, payload.properties, credentials)?;
    let account = state.client.payout_accounts().create(&params).await?;
    Ok((StatusCode::CREATED, Json(account)))
}

/// Handles `GET /payout_accounts` by listing registered payout source accounts.
async fn list_payout_accounts(
    State(state): State<AppState>,
) -> Result<Json<PaginatedResponse<PayoutAccount>>, AppError> {
    let list = state
        .client
        .payout_accounts()
        .list(&ListPayoutAccountsParams::new())
        .await?;
    Ok(Json(PaginatedResponse::from(list)))
}

/// Handles `POST /payouts` by initiating a single payout disbursement.
async fn create_payout(
    State(state): State<AppState>,
    Json(params): Json<CreatePayoutParams>,
) -> Result<(StatusCode, Json<Payout>), AppError> {
    let payout = state.client.payouts().create(&params).await?;
    Ok((StatusCode::CREATED, Json(payout)))
}

/// Handles `GET /payouts/{id}` by retrieving a single payout disbursement by ID.
async fn retrieve_payout(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Payout>, AppError> {
    let payout_id = PayoutId::new(id)?;
    let payout = state.client.payouts().retrieve(&payout_id).await?;
    Ok(Json(payout))
}

/// Handles `GET /settlements/balance` by fetching the current merchant balance.
async fn fetch_settlement_balance(
    State(state): State<AppState>,
) -> Result<Json<MerchantBalance>, AppError> {
    let balance = state.client.settlements().fetch_balance().await?;
    Ok(Json(balance))
}

/// Handles `GET /settlements/{id}` by retrieving a single settlement record by ID.
async fn retrieve_settlement(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Settlement>, AppError> {
    let settlement_id = SettlementId::new(id);
    settlement_id.validate()?;
    let settlement = state.client.settlements().retrieve(&settlement_id).await?;
    Ok(Json(settlement))
}

/// Handles `GET /transfers` by listing merchant ledger transfers.
async fn list_transfers(
    State(state): State<AppState>,
) -> Result<Json<PaginatedResponse<Transfer>>, AppError> {
    let list = state
        .client
        .transfers()
        .list(&ListTransfersParams::new())
        .await?;
    Ok(Json(PaginatedResponse::from(list)))
}

/// Handles `GET /transfers/{id}` by retrieving a single ledger transfer by ID.
async fn retrieve_transfer(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Transfer>, AppError> {
    let transfer_id = TransferId::new(id);
    transfer_id.validate()?;
    let transfer = state.client.transfers().retrieve(&transfer_id).await?;
    Ok(Json(transfer))
}

/// Handles `GET /internal_transactions` by listing internal ledger transactions.
async fn list_internal_transactions(
    State(state): State<AppState>,
) -> Result<Json<PaginatedResponse<InternalTransaction>>, AppError> {
    let list = state
        .client
        .internal_transactions()
        .list(&ListInternalTransactionsParams::new())
        .await?;
    Ok(Json(PaginatedResponse::from(list)))
}
