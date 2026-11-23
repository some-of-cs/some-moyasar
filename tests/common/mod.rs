#![allow(dead_code)]

use std::time::Duration;

use serde_json::{Value, json};
use some_moyasar::prelude::*;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub const TEST_SK: &str = "sk_test_integration_secret_12345";
pub const TEST_PK: &str = "pk_test_integration_publishable_12345";
pub const LIVE_SK: &str = "sk_live_integration_secret_99999";
pub const LIVE_PK: &str = "pk_live_integration_publishable_99999";

pub type Route<'a> = (&'a str, &'a str);

fn is_configured_real_secret(raw: &str) -> bool {
    !raw.is_empty() && !raw.contains("replace_with")
}

fn configured_base_url() -> String {
    resolve_env_secret("$MOYASAR_BASE_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
}

/// Attempts to build a real `MoyasarClient<SecretKey>` by passing `"$MOYASAR_SECRET_KEY"`
/// directly to `MoyasarClient::builder("$MOYASAR_SECRET_KEY")` (resolved behind the scenes).
pub fn try_dotenv_secret_client() -> Option<MoyasarClient<SecretKey>> {
    let probe = MoyasarClient::new("$MOYASAR_SECRET_KEY").ok()?;
    if probe.key().mode() != AuthMode::Test
        || !is_configured_real_secret(probe.key().expose_secret())
    {
        return None;
    }
    Some(
        MoyasarClient::builder("$MOYASAR_SECRET_KEY")
            .base_url(configured_base_url())
            .build()
            .expect("configured MOYASAR_BASE_URL must build valid MoyasarClient"),
    )
}

/// Attempts to build a real `MoyasarClient<PublishableKey>` by passing
/// `"$MOYASAR_PUBLISHABLE_KEY"` directly to `MoyasarClient::publishable_builder`.
pub fn try_dotenv_publishable_client() -> Option<MoyasarClient<PublishableKey>> {
    let probe = MoyasarClient::with_publishable_key("$MOYASAR_PUBLISHABLE_KEY").ok()?;
    if !is_configured_real_secret(probe.key().expose_secret()) {
        return None;
    }
    Some(
        MoyasarClient::publishable_builder("$MOYASAR_PUBLISHABLE_KEY")
            .base_url(configured_base_url())
            .build()
            .expect("configured MOYASAR_BASE_URL must build valid MoyasarClient"),
    )
}

/// Returns the active test secret key reference (`"$MOYASAR_SECRET_KEY"` when a real test key
/// is configured in `.env`, otherwise [`TEST_SK`]).
pub fn test_secret_key_ref() -> &'static str {
    match try_dotenv_secret_client() {
        Some(client) if client.key().mode() == AuthMode::Test => "$MOYASAR_SECRET_KEY",
        _ => TEST_SK,
    }
}

/// Returns the active test publishable key reference (`"$MOYASAR_PUBLISHABLE_KEY"` when a real
/// test key is configured in `.env`, otherwise [`TEST_PK`]).
pub fn test_publishable_key_ref() -> &'static str {
    match try_dotenv_publishable_client() {
        Some(client) if client.key().mode() == AuthMode::Test => "$MOYASAR_PUBLISHABLE_KEY",
        _ => TEST_PK,
    }
}

/// Returns `true` when `MOYASAR_RUN_LIVE_TESTS` is explicitly set to `"1"` or `"true"`
/// and a real test-mode `MOYASAR_SECRET_KEY` is present in `.env` / environment.
pub fn is_live_dotenv_test_enabled() -> bool {
    let enabled = resolve_env_secret("$MOYASAR_RUN_LIVE_TESTS")
        .ok()
        .is_some_and(|v| {
            let trimmed = v.trim();
            trimmed == "1" || trimmed.eq_ignore_ascii_case("true")
        });
    enabled && try_dotenv_secret_client().is_some()
}

pub async fn start_secret_client() -> (MockServer, MoyasarClient<SecretKey>) {
    start_secret_client_with_retries(1).await
}

pub async fn start_secret_client_with_retries(
    retries: u32,
) -> (MockServer, MoyasarClient<SecretKey>) {
    let server = MockServer::start().await;
    let client = MoyasarClient::builder(test_secret_key_ref())
        .base_url(server.uri())
        .timeout(Duration::from_millis(400))
        .max_network_retries(retries)
        .build()
        .expect("loopback test secret client should build");
    (server, client)
}

pub async fn start_publishable_client() -> (MockServer, MoyasarClient<PublishableKey>) {
    let server = MockServer::start().await;
    let client = MoyasarClient::publishable_builder(test_publishable_key_ref())
        .base_url(server.uri())
        .timeout(Duration::from_millis(400))
        .max_network_retries(1)
        .build()
        .expect("loopback test publishable client should build");
    (server, client)
}

async fn mount_template(server: &MockServer, route: Route<'_>, template: ResponseTemplate) {
    Mock::given(method(route.0))
        .and(path(route.1))
        .and(header("moyasar-version", API_VERSION))
        .respond_with(template)
        .mount(server)
        .await;
}

pub async fn mount_json_endpoint(srv: &MockServer, route: Route<'_>, status: u16, body: Value) {
    let tpl = ResponseTemplate::new(status).set_body_json(body);
    mount_template(srv, route, tpl).await;
}

pub async fn mount_raw_endpoint(srv: &MockServer, route: Route<'_>, status: u16, raw: &str) {
    let tpl = ResponseTemplate::new(status).set_body_string(raw);
    mount_template(srv, route, tpl).await;
}

pub fn paginated_envelope(
    key: &str,
    items: Vec<Value>,
    current_page: u32,
    next_page: Option<u32>,
) -> Value {
    json!({
        key: items,
        "meta": {
            "current_page": current_page,
            "next_page": next_page,
            "prev_page": current_page.checked_sub(1).filter(|&p| p >= 1),
            "total_pages": next_page.unwrap_or(current_page),
            "total_count": 2
        }
    })
}

pub fn api_error_body(error_type: &str, message: &str, response_code: Option<&str>) -> Value {
    json!({
        "type": error_type,
        "message": message,
        "errors": [
            { "field": "amount", "message": "must be greater than 100" }
        ],
        "response_code": response_code
    })
}
