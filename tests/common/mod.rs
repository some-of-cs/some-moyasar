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

fn parse_configured_key<K: AuthKey>(
    env_ref: &str,
    parse: fn(&str) -> Result<K, MoyasarError>,
) -> Option<K> {
    parse(env_ref)
        .ok()
        .filter(|k| is_configured_real_secret(k.expose_secret()))
}

fn resolve_test_key<K: AuthKey>(
    env_ref: &str,
    fallback: &str,
    parse: fn(&str) -> Result<K, MoyasarError>,
) -> K {
    parse_configured_key(env_ref, parse)
        .filter(|k| k.mode() == AuthMode::Test)
        .unwrap_or_else(|| parse(fallback).expect("valid fallback test key"))
}

/// Loads `MOYASAR_SECRET_KEY` from the process environment or `.env.local` / `.env`
/// using the SDK's built-in `$VAR` resolver (`env-keys` feature), with fallback to [`TEST_SK`].
pub fn resolve_test_secret_key() -> SecretKey {
    resolve_test_key("$MOYASAR_SECRET_KEY", TEST_SK, SecretKey::new)
}

/// Loads `MOYASAR_PUBLISHABLE_KEY` from the process environment or `.env.local` / `.env`
/// using the SDK's built-in `$VAR` resolver (`env-keys` feature), with fallback to [`TEST_PK`].
pub fn resolve_test_publishable_key() -> PublishableKey {
    resolve_test_key("$MOYASAR_PUBLISHABLE_KEY", TEST_PK, PublishableKey::new)
}

fn configured_base_url() -> String {
    std::env::var("MOYASAR_BASE_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
}

/// Attempts to build a real `MoyasarClient<SecretKey>` from `MOYASAR_SECRET_KEY` in `.env`
/// or the process environment targeting `https://api.moyasar.com` (or `$MOYASAR_BASE_URL`).
pub fn try_dotenv_secret_client() -> Option<MoyasarClient<SecretKey>> {
    let key = parse_configured_key("$MOYASAR_SECRET_KEY", SecretKey::new)?;
    MoyasarClient::builder(key)
        .base_url(configured_base_url())
        .build()
        .ok()
}

/// Attempts to build a real `MoyasarClient<PublishableKey>` from `MOYASAR_PUBLISHABLE_KEY`
/// in `.env` or the process environment targeting `https://api.moyasar.com`.
pub fn try_dotenv_publishable_client() -> Option<MoyasarClient<PublishableKey>> {
    parse_configured_key("$MOYASAR_PUBLISHABLE_KEY", PublishableKey::new)
        .and_then(|key| MoyasarClient::new(key).ok())
}

/// Returns `true` when a real `MOYASAR_SECRET_KEY` is present in `.env` / environment
/// and `MOYASAR_RUN_LIVE_TESTS` is not explicitly set to `"0"` or `"false"`.
pub fn is_live_dotenv_test_enabled() -> bool {
    let disabled = std::env::var("MOYASAR_RUN_LIVE_TESTS")
        .ok()
        .is_some_and(|v| v == "0" || v.eq_ignore_ascii_case("false"));
    !disabled && try_dotenv_secret_client().is_some()
}

async fn start_client_with_key<K: AuthKey>(key: K, retries: u32) -> (MockServer, MoyasarClient<K>) {
    let server = MockServer::start().await;
    let client = MoyasarClient::builder(key)
        .base_url(server.uri())
        .timeout(Duration::from_millis(400))
        .max_network_retries(retries)
        .build()
        .expect("loopback test client should build");
    (server, client)
}

pub async fn start_secret_client() -> (MockServer, MoyasarClient<SecretKey>) {
    start_secret_client_with_retries(1).await
}

pub async fn start_secret_client_with_retries(
    retries: u32,
) -> (MockServer, MoyasarClient<SecretKey>) {
    start_client_with_key(resolve_test_secret_key(), retries).await
}

pub async fn start_publishable_client() -> (MockServer, MoyasarClient<PublishableKey>) {
    start_client_with_key(resolve_test_publishable_key(), 1).await
}

async fn mount_template(server: &MockServer, route: (&str, &str), template: ResponseTemplate) {
    Mock::given(method(route.0))
        .and(path(route.1))
        .and(header("moyasar-version", API_VERSION))
        .respond_with(template)
        .mount(server)
        .await;
}

pub async fn mount_json_endpoint(
    server: &MockServer,
    route: (&str, &str),
    status: u16,
    body: Value,
) {
    mount_template(
        server,
        route,
        ResponseTemplate::new(status).set_body_json(body),
    )
    .await;
}

pub async fn mount_raw_endpoint(
    server: &MockServer,
    route: (&str, &str),
    status: u16,
    raw_body: &str,
) {
    mount_template(
        server,
        route,
        ResponseTemplate::new(status).set_body_string(raw_body),
    )
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
