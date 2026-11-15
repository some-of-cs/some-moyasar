//! Rocket routes for Moyasar invoices and SAMA-regulated payouts.

use chrono::{DateTime, Utc};
use rocket::State;
use rocket::serde::json::Json;
use serde::{Deserialize, Serialize};
use some_moyasar::kernel::error::{ApiErrorType, MoyasarError};
use some_moyasar::kernel::types::{Amount, Currency};
use some_moyasar::resources::invoices::cancel::sync_sadad_expiry;
use some_moyasar::resources::invoices::create::CreateInvoiceParams;
use some_moyasar::resources::invoices::models::Invoice;
use some_moyasar::resources::payouts::destinations::PayoutDestination;
use some_moyasar::resources::payouts::models::PayoutAccountId;
use some_moyasar::resources::payouts::payout_model::Payout;
use some_moyasar::resources::payouts::payouts_create::CreatePayoutParams;
use some_moyasar::resources::payouts::purpose::PayoutPurpose;
use uuid::Uuid;

use crate::responder::MoyasarResponder;
use crate::state::RocketAppState;

/// Request payload for creating a hosted Moyasar e-invoice via Rocket.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateInvoiceRequest {
    /// Invoice amount in minor currency units (for example, Halalas for SAR).
    pub amount: i64,
    /// Three-letter ISO 4217 currency code.
    pub currency: Currency,
    /// Optional human-readable description shown on the hosted invoice page.
    pub description: Option<String>,
    /// Optional webhook URL invoked when the invoice payment state changes.
    pub callback_url: Option<String>,
    /// Optional UTC expiration timestamp for the hosted invoice.
    pub expired_at: Option<DateTime<Utc>>,
}

/// Response payload returned after canceling a hosted Moyasar invoice.
#[derive(Debug, Clone, Serialize)]
pub struct CancelInvoiceResponse {
    /// The canceled Moyasar invoice resource.
    pub invoice: Invoice,
    /// Synchronized SADAD bill expiration timestamp (`min(expired_at, now + 29 days)`).
    pub sadad_expires_at: DateTime<Utc>,
}

/// Request payload for initiating a SAMA-regulated payout via Rocket.
#[derive(Debug, Clone, Deserialize)]
pub struct CreatePayoutRequest {
    /// Identifier of the source payout account funding the disbursement.
    pub source_id: String,
    /// Disbursement amount in minor currency units (must be greater than zero).
    pub amount: u64,
    /// SAMA-regulated 20-variant payout purpose code.
    pub purpose: PayoutPurpose,
    /// Destination bank account or digital wallet receiving the payout.
    pub destination: PayoutDestination,
    /// Optional 16-digit ASCII numeric sequence number for caller tracking.
    pub sequence_number: Option<String>,
    /// Optional human-readable comment attached to the payout.
    pub comment: Option<String>,
}

fn parse_optional_utc(raw: Option<&str>) -> Option<DateTime<Utc>> {
    raw.and_then(|s| DateTime::parse_from_rfc3339(s).ok().map(|dt| dt.with_timezone(&Utc)))
}

/// Creates a hosted Moyasar e-invoice (`POST /invoices`).
///
/// Converts integer minor-unit `amount` into [`Amount`] without floating-point
/// arithmetic and forwards the request to [`InvoicesService::create`].
#[rocket::post("/invoices", format = "json", data = "<req>")]
pub async fn create_invoice(
    state: &State<RocketAppState>,
    req: Json<CreateInvoiceRequest>,
) -> Result<Json<Invoice>, MoyasarResponder> {
    let payload = req.into_inner();
    let mut params = CreateInvoiceParams::new(Amount::from_minor(payload.amount), payload.currency);
    params.description = payload.description;
    params.callback_url = payload.callback_url;
    params.expired_at = payload.expired_at;

    let invoice = state
        .client
        .invoices()
        .create(&params)
        .await
        .map_err(MoyasarResponder)?;
    Ok(Json(invoice))
}

/// Cancels a hosted Moyasar e-invoice and computes its synchronized SADAD expiry (`POST /invoices/<id>/cancel`).
///
/// Parses `id` as a [`Uuid`], delegates cancellation to [`InvoicesService::cancel`],
/// and computes `sadad_expires_at` using [`sync_sadad_expiry`].
#[rocket::post("/invoices/<id>/cancel")]
pub async fn cancel_invoice(
    state: &State<RocketAppState>,
    id: &str,
) -> Result<Json<CancelInvoiceResponse>, MoyasarResponder> {
    let uuid = Uuid::parse_str(id).map_err(|err| {
        MoyasarResponder(MoyasarError::Api {
            error_type: ApiErrorType::InvalidRequestError,
            message: format!("invalid invoice UUID: {err}"),
            errors: Vec::new(),
            response_code: None,
        })
    })?;

    let invoice = state
        .client
        .invoices()
        .cancel(uuid)
        .await
        .map_err(MoyasarResponder)?;
    let expired_at = parse_optional_utc(invoice.expired_at.as_deref());
    let sadad_expires_at = sync_sadad_expiry(expired_at, Utc::now());

    Ok(Json(CancelInvoiceResponse {
        invoice,
        sadad_expires_at,
    }))
}

/// Initiates a SAMA-regulated single payout (`POST /payouts`).
///
/// Validates the [`PayoutPurpose`] code at deserialization, constructs
/// [`CreatePayoutParams`], runs [`CreatePayoutParams::validate`] (enforcing
/// `amount > 0` and `SA` + 22-digit IBAN rules), and submits the disbursement
/// via [`PayoutsService::create`].
#[rocket::post("/payouts", format = "json", data = "<req>")]
pub async fn create_payout(
    state: &State<RocketAppState>,
    req: Json<CreatePayoutRequest>,
) -> Result<Json<Payout>, MoyasarResponder> {
    let payload = req.into_inner();
    let source_id = PayoutAccountId::new(payload.source_id).map_err(MoyasarResponder)?;
    let params = CreatePayoutParams {
        source_id,
        sequence_number: payload.sequence_number,
        amount: payload.amount,
        purpose: payload.purpose,
        destination: payload.destination,
        comment: payload.comment,
        metadata: None,
    };
    params.validate().map_err(MoyasarResponder)?;

    let payout = state
        .client
        .payouts()
        .create(&params)
        .await
        .map_err(MoyasarResponder)?;
    Ok(Json(payout))
}

/// Returns all Rocket routes for invoices and SAMA-regulated payouts.
pub fn routes() -> Vec<rocket::Route> {
    rocket::routes![create_invoice, cancel_invoice, create_payout]
}
