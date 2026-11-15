//! Shared Rocket async tracked client harness and E2E test modules.
//!
//! Note for Story 2 integration: once `examples/rocket/*` are merged,
//! `examples/rocket/routes_*.rs` reference `crate::state` and
//! `crate::responder`, so `tests/rocket_integration.rs` will need crate-root
//! `#[path]` declarations (or `pub use rocket_suite::{state, responder};`)
//! alongside mounting the example route modules directly.

#![warn(missing_docs)]

use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use rocket::http::Status;
use rocket::local::asynchronous::Client;
use rocket::serde::{Deserialize, json::Json};
use rocket::{Build, Rocket, State, post, routes};
use serde_json::{Value, json};
use some_moyasar::prelude::*;
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

#[path = "../../examples/rocket/responder.rs"]
pub mod responder;
#[path = "../../examples/rocket/routes_invoices_payouts.rs"]
pub mod routes_invoices_payouts;
#[path = "../../examples/rocket/routes_settlements.rs"]
pub mod routes_settlements;
#[path = "../../examples/rocket/routes_webhooks.rs"]
pub mod routes_webhooks;
#[path = "../../examples/rocket/state.rs"]
pub mod state;

mod invoices_payouts_test;
mod settlements_test;
mod webhooks_test;

/// Managed Rocket test application state holding a loopback-wired SDK client.
pub struct RocketTestState {
    /// Authenticated Moyasar secret client targeting [`MoyasarMockServer`].
    pub client: MoyasarClient<SecretKey>,
}

/// JSON request payload accepted by the test `POST /invoices` Rocket route.
#[derive(Debug, Deserialize)]
pub struct InvoiceCreateRequest {
    /// Invoice amount in minor currency units (e.g. Halalas for SAR).
    pub amount: Amount,
    /// ISO 4217 currency code.
    pub currency: Currency,
    /// Optional human-readable invoice description.
    pub description: Option<String>,
    /// Optional callback URL for invoice status webhooks.
    pub callback_url: Option<String>,
}

/// Maps a [`MoyasarError`] to an appropriate HTTP [`Status`] for test routes.
fn map_error_status(err: &MoyasarError) -> Status {
    match err {
        MoyasarError::InvalidIban | MoyasarError::InvalidPayoutAmount => Status::BadRequest,
        _ => Status::BadGateway,
    }
}

/// Formats a redacted JSON error response with the provided HTTP [`Status`].
fn error_response(status: Status, err: &MoyasarError) -> (Status, Json<Value>) {
    (status, Json(json!({ "error": err.to_string() })))
}

/// Test Rocket handler for `POST /invoices`.
#[post("/invoices", data = "<payload>")]
async fn create_invoice(
    state: &State<RocketTestState>,
    payload: Json<InvoiceCreateRequest>,
) -> (Status, Json<Value>) {
    let req = payload.into_inner();
    let mut params = CreateInvoiceParams::new(req.amount, req.currency);
    params.description = req.description;
    params.callback_url = req.callback_url;

    match state.client.invoices().create(&params).await {
        Ok(invoice) => (Status::Created, Json(json!(invoice))),
        Err(err) => error_response(map_error_status(&err), &err),
    }
}

/// Test Rocket handler for `POST /invoices/<id>/cancel` with 29-day SADAD sync.
#[post("/invoices/<id>/cancel")]
async fn cancel_invoice(state: &State<RocketTestState>, id: &str) -> (Status, Json<Value>) {
    let Ok(invoice_id) = Uuid::parse_str(id) else {
        return (
            Status::BadRequest,
            Json(json!({ "error": "invalid invoice uuid" })),
        );
    };

    match state.client.invoices().cancel(invoice_id).await {
        Ok(invoice) => {
            let now = Utc::now();
            let synced = sync_sadad_expiry(Some(now + ChronoDuration::days(35)), now);
            (
                Status::Ok,
                Json(json!({
                    "invoice": invoice,
                    "sadad_synced_expiry": synced.to_rfc3339(),
                    "max_sadad_expiry_days": MAX_SADAD_EXPIRY_DAYS,
                })),
            )
        }
        Err(err) => error_response(map_error_status(&err), &err),
    }
}

/// Test Rocket handler for `POST /payouts` with SAMA IBAN and purpose validation.
#[post("/payouts", data = "<payload>")]
async fn create_payout(
    state: &State<RocketTestState>,
    payload: Json<CreatePayoutParams>,
) -> (Status, Json<Value>) {
    match state.client.payouts().create(&payload.into_inner()).await {
        Ok(payout) => (Status::Created, Json(json!(payout))),
        Err(err) => error_response(map_error_status(&err), &err),
    }
}

/// Builds a [`Rocket`] instance managed with [`RocketTestState`] and E2E routes.
#[must_use]
pub fn build_test_rocket(client: MoyasarClient<SecretKey>) -> Rocket<Build> {
    rocket::build()
        .manage(RocketTestState { client })
        .mount("/", routes![create_invoice, cancel_invoice, create_payout])
}

/// Spawns a [`MoyasarMockServer`] and a tracked Rocket [`Client`] wired to it.
pub async fn spawn_test_client() -> (MoyasarMockServer, Client) {
    let server = MoyasarMockServer::start().await;
    let sdk_client = MoyasarClient::builder(
        SecretKey::new("sk_test_rocket_e2e_12345").expect("valid test secret key"),
    )
    .base_url(server.uri())
    .timeout(Duration::from_millis(500))
    .max_network_retries(0)
    .build()
    .expect("loopback mock client should build");

    let rocket_client = Client::tracked(build_test_rocket(sdk_client))
        .await
        .expect("tracked Rocket client should initialize");

    (server, rocket_client)
}

/// Mounts a deterministic JSON response on [`MoyasarMockServer`] for `moyasar-version`.
pub async fn mount_json_endpoint(
    server: &MoyasarMockServer,
    http_method: &str,
    endpoint_path: &str,
    status: u16,
    body: Value,
) {
    Mock::given(method(http_method))
        .and(path(endpoint_path))
        .and(header("moyasar-version", API_VERSION))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(server.server())
        .await;
}
