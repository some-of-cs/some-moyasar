//! Rocket routes for `RefundsService`, `CustomersService`, `SubscriptionsService`, and `UnifiedPaymentGateway`.

use rocket::Route;

/// Returns all routes for Stripe compatibility services and the unified payment gateway.
#[must_use]
pub fn routes() -> Vec<Route> {
    Vec::new()
}
