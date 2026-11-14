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

fn is_configured_real_secret(raw: &str) -> bool {
    !raw.is_empty() && !raw.contains("replace_with")
}

/// Loads `MOYASAR_SECRET_KEY` from the process environment or `.env.local` / `.env`
/// using the SDK's built-in `$VAR` resolver (`env-keys` feature).
/// Falls back to [`TEST_SK`] when no `.env` key is configured (e.g., in CI) or when
/// the configured key is a live key and a test-mode loopback client is required.
pub fn resolve_test_secret_key() -> SecretKey {
    if let Ok(key) = SecretKey::new("$MOYASAR_SECRET_KEY")
        && key.mode() == AuthMode::Test
        && is_configured_real_secret(key.expose_secret())
    {
        return key;
    }
    SecretKey::new(TEST_SK).expect("valid fallback test secret key")
}

/// Loads `MOYASAR_PUBLISHABLE_KEY` from the process environment or `.env.local` / `.env`
/// using the SDK's built-in `$VAR` resolver (`env-keys` feature).
/// Falls back to [`TEST_PK`] when no `.env` key is configured.
pub fn resolve_test_publishable_key() -> PublishableKey {
    if let Ok(key) = PublishableKey::new("$MOYASAR_PUBLISHABLE_KEY")
        && key.mode() == AuthMode::Test
        && is_configured_real_secret(key.expose_secret())
    {
        return key;
    }
    PublishableKey::new(TEST_PK).expect("valid fallback test publishable key")
}

/// Attempts to build a real `MoyasarClient<SecretKey>` from `MOYASAR_SECRET_KEY` in `.env`
/// or the process environment targeting `https://api.moyasar.com` (or `$MOYASAR_BASE_URL`).
/// Returns `None` if `MOYASAR_SECRET_KEY` is not set or still holds the `.env.example` placeholder.
pub fn try_dotenv_secret_client() -> Option<MoyasarClient<SecretKey>> {
    let key = SecretKey::new("$MOYASAR_SECRET_KEY").ok()?;
    if !is_configured_real_secret(key.expose_secret()) {
        return None;
    }
    let base_url = std::env::var("MOYASAR_BASE_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    MoyasarClient::builder(key).base_url(base_url).build().ok()
}

/// Attempts to build a real `MoyasarClient<PublishableKey>` from `MOYASAR_PUBLISHABLE_KEY`
/// in `.env` or the process environment targeting `https://api.moyasar.com`.
pub fn try_dotenv_publishable_client() -> Option<MoyasarClient<PublishableKey>> {
    let key = PublishableKey::new("$MOYASAR_PUBLISHABLE_KEY").ok()?;
    if !is_configured_real_secret(key.expose_secret()) {
        return None;
    }
    MoyasarClient::new(key).ok()
}

/// Returns `true` when a real `MOYASAR_SECRET_KEY` is present in `.env` / environment
/// and `MOYASAR_RUN_LIVE_TESTS` is not explicitly set to `"0"` or `"false"`.
pub fn is_live_dotenv_test_enabled() -> bool {
    let disabled = std::env::var("MOYASAR_RUN_LIVE_TESTS")
        .ok()
        .is_some_and(|v| v == "0" || v.eq_ignore_ascii_case("false"));
    !disabled && try_dotenv_secret_client().is_some()
}

pub async fn start_secret_client() -> (MockServer, MoyasarClient<SecretKey>) {
    start_secret_client_with_retries(1).await
}

pub async fn start_secret_client_with_retries(
    retries: u32,
) -> (MockServer, MoyasarClient<SecretKey>) {
    let server = MockServer::start().await;
    let key = resolve_test_secret_key();
    let client = MoyasarClient::builder(key)
        .base_url(server.uri())
        .timeout(Duration::from_millis(400))
        .max_network_retries(retries)
        .build()
        .expect("loopback test client should build");
    (server, client)
}

pub async fn start_publishable_client() -> (MockServer, MoyasarClient<PublishableKey>) {
    let server = MockServer::start().await;
    let key = resolve_test_publishable_key();
    let client = MoyasarClient::builder(key)
        .base_url(server.uri())
        .timeout(Duration::from_millis(400))
        .max_network_retries(1)
        .build()
        .expect("loopback publishable test client should build");
    (server, client)
}

pub async fn mount_json_endpoint(
    server: &MockServer,
    http_method: &str,
    endpoint_path: &str,
    status: u16,
    body: Value,
) {
    Mock::given(method(http_method))
        .and(path(endpoint_path))
        .and(header("moyasar-version", API_VERSION))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(server)
        .await;
}

pub async fn mount_raw_endpoint(
    server: &MockServer,
    http_method: &str,
    endpoint_path: &str,
    status: u16,
    raw_body: &str,
) {
    Mock::given(method(http_method))
        .and(path(endpoint_path))
        .respond_with(ResponseTemplate::new(status).set_body_string(raw_body))
        .mount(server)
        .await;
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
