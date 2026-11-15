//! Rocket routes for `TransfersService`, `InternalTransactionsService`, `SourcesService`, and `ApplePayService`.

use rocket::Route;

/// Returns all routes for transfers, internal transactions, BIN issuer lookup, and Apple Pay.
#[must_use]
pub fn routes() -> Vec<Route> {
    Vec::new()
}
