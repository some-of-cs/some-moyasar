//! Rocket routes for Moyasar settlements and merchant balance.

use rocket::State;
use rocket::serde::json::Json;
use serde::Serialize;

use some_moyasar::prelude::{
    Amount, ListSettlementsParams, MerchantBalance, Settlement, SettlementId,
};

use crate::responder::MoyasarResponder;
use crate::state::RocketAppState;

/// Serializable response payload for the paginated settlements listing endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct SettlementsListResponse {
    /// Settlement records returned on the current page.
    pub items: Vec<Settlement>,
    /// Total number of matching settlement records across all pages.
    pub total_count: Option<u64>,
    /// Current 1-based page number.
    pub current_page: Option<u32>,
    /// Next page number, if more results exist.
    pub next_page: Option<u32>,
}

/// Response payload pairing a [`Settlement`] with its checked net payout amount.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SettlementBreakdownResponse {
    /// Retrieved Moyasar settlement record.
    pub settlement: Settlement,
    /// Net settled amount (`amount - fee - tax`) in minor units, or `None` on underflow.
    pub net_amount: Option<Amount>,
}

/// Lists Moyasar settlements using default pagination parameters.
///
/// Maps `GET /settlements` to `SettlementsService::list`.
#[rocket::get("/settlements")]
pub async fn list_settlements(
    state: &State<RocketAppState>,
) -> Result<Json<SettlementsListResponse>, MoyasarResponder> {
    let page = state
        .client
        .settlements()
        .list(&ListSettlementsParams::new())
        .await?;
    Ok(Json(SettlementsListResponse {
        items: page.items,
        total_count: Some(page.meta.total_count),
        current_page: Some(page.meta.current_page),
        next_page: page.meta.next_page,
    }))
}

/// Retrieves a single Moyasar settlement by ID along with its checked net amount.
///
/// Maps `GET /settlements/<id>` to `SettlementsService::retrieve` and [`Settlement::checked_net`].
#[rocket::get("/settlements/<id>")]
pub async fn get_settlement(
    state: &State<RocketAppState>,
    id: &str,
) -> Result<Json<SettlementBreakdownResponse>, MoyasarResponder> {
    let sid = SettlementId::new(id);
    sid.validate()?;
    let settlement = state.client.settlements().retrieve(&sid).await?;
    let net_amount = settlement.checked_net();
    Ok(Json(SettlementBreakdownResponse {
        settlement,
        net_amount,
    }))
}

/// Fetches the current merchant settlement balance.
///
/// Maps `GET /settlements/balance` to `SettlementsService::fetch_balance`.
#[rocket::get("/settlements/balance")]
pub async fn get_balance(
    state: &State<RocketAppState>,
) -> Result<Json<MerchantBalance>, MoyasarResponder> {
    let balance = state.client.settlements().fetch_balance().await?;
    Ok(Json(balance))
}

/// Returns all Rocket routes for the settlements and merchant balance endpoints.
#[must_use]
pub fn routes() -> Vec<rocket::Route> {
    rocket::routes![list_settlements, get_settlement, get_balance]
}
