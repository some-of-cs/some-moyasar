//! Rocket.rs integration example server for `some-moyasar`.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod responder;
pub mod routes_invoices_payouts;
pub mod routes_settlements;
pub mod routes_webhooks;
pub mod state;

#[rocket::main]
async fn main() -> Result<(), Box<rocket::Error>> {
    Ok(())
}
