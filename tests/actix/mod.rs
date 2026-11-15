//! Actix-Web integration test harness wired to `MoyasarMockServer`.

#[path = "../../examples/actix/error.rs"]
pub mod error;
#[path = "../../examples/actix/handlers_payments.rs"]
pub mod handlers_payments;
#[path = "../../examples/actix/handlers_subscriptions.rs"]
pub mod handlers_subscriptions;
#[path = "../../examples/actix/handlers_webhooks.rs"]
pub mod handlers_webhooks;
#[path = "../../examples/actix/state.rs"]
pub mod state;

mod payments_vault_test;
mod subscriptions_test;
mod webhooks_test;

use actix_web::web;
use serde_json::{Value, json};
use some_moyasar::prelude::{MoyasarClient, MoyasarMockServer, SecretKey};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

/// Test webhook secret used by the Actix-Web E2E test harness.
const TEST_WEBHOOK_SECRET: &str = "whsec_actix_test_secret";

/// Registers all Actix-Web example routes on the given [`web::ServiceConfig`].
pub fn configure_test_routes(cfg: &mut web::ServiceConfig) {
    handlers_payments::configure(cfg);
    handlers_subscriptions::configure(cfg);
    handlers_webhooks::configure(cfg);
}

/// Starts a hermetic [`MoyasarMockServer`] with default routes and builds shared [`state::ActixAppState`].
pub async fn build_test_harness() -> (MoyasarMockServer, web::Data<state::ActixAppState>) {
    let server = MoyasarMockServer::start().await;
    server.mount_default_routes().await;
    let key = SecretKey::new("sk_test_actix_harness_key").expect("valid test secret key");
    let client = MoyasarClient::builder(key)
        .base_url(server.uri())
        .build()
        .expect("loopback mock server URL should build MoyasarClient");
    let app_state = web::Data::new(state::ActixAppState::new(client, TEST_WEBHOOK_SECRET));
    (server, app_state)
}

/// Mounts a JSON responder on `mock_server` for `(http_method, endpoint_path)`.
pub async fn mount_json_endpoint(
    mock_server: &MoyasarMockServer,
    endpoint: (&str, &str),
    status: u16,
    body: Value,
) {
    Mock::given(method(endpoint.0))
        .and(path(endpoint.1))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .with_priority(1)
        .mount(mock_server.server())
        .await;
}

/// Wraps a JSON array of `items` in the Moyasar `PaginatedList` wire envelope.
pub fn paginated_envelope(items: Value) -> Value {
    json!({
        "items": items,
        "meta": {
            "current_page": 1,
            "next_page": null,
            "prev_page": null,
            "total_pages": 1,
            "total_count": 1
        }
    })
}
