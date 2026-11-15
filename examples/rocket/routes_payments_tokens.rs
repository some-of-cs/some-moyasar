//! Rocket routes for `PaymentsService`, `PaymentIntentsService`, `TokensService`, and `CardAuthsService`.

use rocket::Route;

/// Returns all routes for payments, payment intents, tokens, and card authentications.
#[must_use]
pub fn routes() -> Vec<Route> {
    Vec::new()
}
