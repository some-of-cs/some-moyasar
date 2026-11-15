//! Thread-safe `MoyasarClient` backed by `Arc<ClientInner<K>>`.

use core::fmt;
use std::sync::Arc;
use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};

use crate::kernel::auth::{AuthKey, AuthMode, PublishableKey, SecretKey};
use crate::kernel::error::MoyasarError;
use crate::kernel::transport::build_default_headers;

/// Default Moyasar API base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.moyasar.com";

/// Converts a raw key string (`"sk_test_..."`), an environment/dotenv variable reference
/// (`"$MOYASAR_SECRET_KEY"` or `"${MOYASAR_SECRET_KEY}"`), or an existing [`AuthKey`]
/// into a validated key of type `K` behind the scenes when constructing a [`MoyasarClient`].
pub trait IntoClientKey<K: AuthKey> {
    /// Resolves and validates the key material.
    fn into_client_key(self) -> Result<K, MoyasarError>;
}

impl<K: AuthKey> IntoClientKey<K> for K {
    fn into_client_key(self) -> Result<K, MoyasarError> {
        Ok(self)
    }
}

impl IntoClientKey<SecretKey> for &str {
    fn into_client_key(self) -> Result<SecretKey, MoyasarError> {
        SecretKey::new(self)
    }
}

impl IntoClientKey<SecretKey> for String {
    fn into_client_key(self) -> Result<SecretKey, MoyasarError> {
        SecretKey::new(&self)
    }
}

impl IntoClientKey<SecretKey> for &String {
    fn into_client_key(self) -> Result<SecretKey, MoyasarError> {
        SecretKey::new(self.as_str())
    }
}

/// Converts a raw publishable key string (`"pk_test_..."`), an environment/dotenv variable
/// reference (`"$MOYASAR_PUBLISHABLE_KEY"` or `"${MOYASAR_PUBLISHABLE_KEY}"`), or an existing
/// [`PublishableKey`] into a validated [`PublishableKey`] behind the scenes.
pub trait IntoPublishableClientKey {
    /// Resolves and validates the publishable key material.
    fn into_publishable_client_key(self) -> Result<PublishableKey, MoyasarError>;
}

impl IntoPublishableClientKey for PublishableKey {
    fn into_publishable_client_key(self) -> Result<PublishableKey, MoyasarError> {
        Ok(self)
    }
}

impl IntoPublishableClientKey for &str {
    fn into_publishable_client_key(self) -> Result<PublishableKey, MoyasarError> {
        PublishableKey::new(self)
    }
}

impl IntoPublishableClientKey for String {
    fn into_publishable_client_key(self) -> Result<PublishableKey, MoyasarError> {
        PublishableKey::new(&self)
    }
}

impl IntoPublishableClientKey for &String {
    fn into_publishable_client_key(self) -> Result<PublishableKey, MoyasarError> {
        PublishableKey::new(self.as_str())
    }
}

/// Configuration shared across all `MoyasarClient` clones.
#[derive(Clone)]
pub struct MoyasarClientConfig {
    /// Base API URL (HTTPS enforced in production).
    pub base_url: String,
    /// Per-request timeout.
    pub timeout: Duration,
    /// Maximum automatic retries for network/5xx errors.
    pub max_network_retries: u32,
    /// Optional shared webhook secret for constant-time webhook verification.
    pub webhook_secret: Option<SecretString>,
}

impl Default for MoyasarClientConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_owned(),
            timeout: Duration::from_secs(30),
            max_network_retries: 2,
            webhook_secret: None,
        }
    }
}

/// Shared inner state for `MoyasarClient`.
pub struct ClientInner<K: AuthKey> {
    key: K,
    http: reqwest::Client,
    config: MoyasarClientConfig,
}

/// Immutable, cheaply cloneable (`Arc`), thread-safe Moyasar API client.
#[derive(Clone)]
pub struct MoyasarClient<K: AuthKey = SecretKey>(Arc<ClientInner<K>>);

/// Builder for configuring a `MoyasarClient`.
pub struct MoyasarClientBuilder<K: AuthKey> {
    key: Result<K, MoyasarError>,
    config: MoyasarClientConfig,
    webhook_secret_invalid: bool,
}

impl<K: AuthKey> MoyasarClient<K> {
    /// Creates a new `MoyasarClient` with default settings, resolving `"$VAR_NAME"` or raw key
    /// strings behind the scenes.
    pub fn new(key: impl IntoClientKey<K>) -> Result<Self, MoyasarError> {
        Self::builder(key).build()
    }

    /// Starts a builder for customizing timeouts, retries, and base URL, resolving `"$VAR_NAME"`
    /// or raw key strings behind the scenes.
    pub fn builder(key: impl IntoClientKey<K>) -> MoyasarClientBuilder<K> {
        MoyasarClientBuilder {
            key: key.into_client_key(),
            config: MoyasarClientConfig::default(),
            webhook_secret_invalid: false,
        }
    }

    /// Returns a reference to the underlying `reqwest::Client`.
    pub fn http(&self) -> &reqwest::Client {
        &self.0.http
    }

    /// Returns a reference to the client configuration.
    pub fn config(&self) -> &MoyasarClientConfig {
        &self.0.config
    }

    /// Returns the configured base URL (without trailing slash).
    pub fn base_url(&self) -> &str {
        &self.0.config.base_url
    }

    /// Returns the configured per-request timeout.
    pub fn timeout(&self) -> Duration {
        self.0.config.timeout
    }

    /// Returns the configured maximum automatic retry count.
    pub fn max_network_retries(&self) -> u32 {
        self.0.config.max_network_retries
    }

    /// Returns a reference to the client's authentication key.
    pub fn key(&self) -> &K {
        &self.0.key
    }

    /// Returns a reference to the configured shared webhook secret, if any.
    #[must_use]
    pub fn webhook_secret(&self) -> Option<&SecretString> {
        self.0.config.webhook_secret.as_ref()
    }
}

impl MoyasarClient<PublishableKey> {
    /// Creates a new publishable `MoyasarClient<PublishableKey>` directly from `"$VAR_NAME"`,
    /// `"pk_test_..."`, or a [`PublishableKey`] without calling `PublishableKey::new` explicitly.
    pub fn with_publishable_key(key: impl IntoPublishableClientKey) -> Result<Self, MoyasarError> {
        Self::publishable_builder(key).build()
    }

    /// Starts a builder for a publishable `MoyasarClient<PublishableKey>` directly from
    /// `"$VAR_NAME"`, `"pk_test_..."`, or a [`PublishableKey`].
    pub fn publishable_builder(
        key: impl IntoPublishableClientKey,
    ) -> MoyasarClientBuilder<PublishableKey> {
        MoyasarClientBuilder {
            key: key.into_publishable_client_key(),
            config: MoyasarClientConfig::default(),
            webhook_secret_invalid: false,
        }
    }
}

/// Validates that `url` is a well-formed `https://` base URL without embedded credentials.
///
/// Loopback `http://127.0.0.1`, `http://localhost`, and `http://[::1]` URLs are also
/// permitted in test mode.
///
/// # Errors
///
/// Returns [`MoyasarError::InvalidBaseUrl`] if `url` fails to parse, contains userinfo,
/// or uses a disallowed scheme or host.
pub fn validate_https_url(url: &str) -> Result<(), MoyasarError> {
    validate_base_url_for_mode(url, AuthMode::Test)
}

fn validate_base_url_for_mode(url: &str, mode: AuthMode) -> Result<(), MoyasarError> {
    let parsed = reqwest::Url::parse(url).map_err(|_| MoyasarError::InvalidBaseUrl)?;
    if !has_clean_authority(&parsed)
        || !has_no_query_or_fragment(&parsed)
        || !is_permitted_scheme_and_host(&parsed, mode)
    {
        return Err(MoyasarError::InvalidBaseUrl);
    }
    Ok(())
}

fn has_clean_authority(parsed: &reqwest::Url) -> bool {
    parsed.username().is_empty() && parsed.password().is_none() && parsed.host_str().is_some()
}

fn has_no_query_or_fragment(parsed: &reqwest::Url) -> bool {
    parsed.query().is_none() && parsed.fragment().is_none()
}

fn is_permitted_scheme_and_host(parsed: &reqwest::Url, mode: AuthMode) -> bool {
    if parsed.scheme() == "https" {
        return true;
    }
    mode == AuthMode::Test
        && parsed.scheme() == "http"
        && matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
}

impl<K: AuthKey> MoyasarClientBuilder<K> {
    /// Overrides the API base URL (must use `https://` without userinfo, or loopback `http://` in `AuthMode::Test`).
    #[must_use]
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.config.base_url = url.into();
        self
    }

    /// Sets the per-request timeout.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    /// Sets the maximum retry count for transient network and 5xx failures.
    #[must_use]
    pub fn max_network_retries(mut self, retries: u32) -> Self {
        self.config.max_network_retries = retries;
        self
    }

    /// Configures the shared webhook secret used by `verify_webhook`, resolving `"$VAR_NAME"`
    /// or `"${VAR_NAME}"` behind the scenes when the `env-keys` feature is enabled.
    #[must_use]
    pub fn webhook_secret(mut self, secret: impl Into<String>) -> Self {
        match resolve_client_webhook_secret(secret.into()) {
            Ok(resolved) => {
                self.config.webhook_secret = Some(resolved);
                self.webhook_secret_invalid = false;
            }
            Err(()) => {
                self.config.webhook_secret = None;
                self.webhook_secret_invalid = true;
            }
        }
        self
    }

    /// Validates configuration and builds the `MoyasarClient`.
    pub fn build(mut self) -> Result<MoyasarClient<K>, MoyasarError> {
        let key = self.key?;
        let trimmed_url = self.config.base_url.trim().trim_end_matches('/').to_owned();
        validate_base_url_for_mode(&trimmed_url, key.mode())?;
        self.config.base_url = trimmed_url;
        if self.webhook_secret_invalid {
            return Err(MoyasarError::InvalidWebhookSecret);
        }
        let headers = build_default_headers(&key)?;
        let http = reqwest::Client::builder()
            .timeout(self.config.timeout)
            .default_headers(headers)
            .https_only(key.mode() == AuthMode::Live)
            .build()?;

        Ok(MoyasarClient(Arc::new(ClientInner {
            key,
            http,
            config: self.config,
        })))
    }
}

#[cfg(feature = "env-keys")]
fn resolve_client_webhook_secret(raw: String) -> Result<SecretString, ()> {
    use zeroize::Zeroize;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(());
    }
    if trimmed.starts_with('$') {
        let mut resolved = crate::kernel::env::resolve_env_secret(trimmed).map_err(|_| ())?;
        if resolved.trim().is_empty() {
            resolved.zeroize();
            return Err(());
        }
        let secret = SecretString::from(resolved.as_str());
        resolved.zeroize();
        return Ok(secret);
    }
    Ok(SecretString::from(raw))
}

#[cfg(not(feature = "env-keys"))]
fn resolve_client_webhook_secret(raw: String) -> Result<SecretString, ()> {
    if raw.trim().is_empty() {
        return Err(());
    }
    Ok(SecretString::from(raw))
}

impl<K: AuthKey> fmt::Debug for MoyasarClient<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MoyasarClient")
            .field("base_url", &self.0.config.base_url)
            .field("key", &"[REDACTED]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_plaintext_http_and_userinfo_base_urls() {
        for bad_url in [
            "http://api.moyasar.com",
            "http://localhost:secret@evil.example.com",
            "https://user:pass@api.moyasar.com",
            "https://user@api.moyasar.com",
            "https://",
            "not a url",
        ] {
            let err = MoyasarClient::builder("sk_test_123")
                .base_url(bad_url)
                .build()
                .expect_err("must reject invalid or userinfo base URL");
            assert!(matches!(err, MoyasarError::InvalidBaseUrl));
        }
    }

    #[test]
    fn rejects_base_url_with_query_or_fragment() {
        for url in [
            "https://api.moyasar.com?foo=bar",
            "https://api.moyasar.com#frag",
        ] {
            let result = MoyasarClient::builder("sk_test_query_url")
                .base_url(url)
                .build();
            assert!(
                matches!(result, Err(MoyasarError::InvalidBaseUrl)),
                "expected InvalidBaseUrl for {url:?}, got {result:?}"
            );
        }
    }

    #[test]
    fn rejects_loopback_http_when_key_is_live_mode() {
        let err = MoyasarClient::builder("sk_live_123")
            .base_url("http://127.0.0.1:8080")
            .build()
            .expect_err("live key must reject loopback http");
        assert!(matches!(err, MoyasarError::InvalidBaseUrl));
    }

    #[test]
    fn arc_clone_shares_inner_and_redacts_debug() {
        let client = MoyasarClient::builder("sk_test_secret_value")
            .webhook_secret("whsec_configured_123")
            .build()
            .expect("client builds");
        let cloned = client.clone();
        assert_eq!(Arc::strong_count(&client.0), 2);
        assert_eq!(
            client.webhook_secret().map(ExposeSecret::expose_secret),
            Some("whsec_configured_123")
        );
        let dbg = format!("{cloned:?}");
        assert!(!dbg.contains("sk_test_secret_value"));
        assert!(!dbg.contains("whsec_configured_123"));
    }

    #[test]
    fn rejects_empty_or_whitespace_webhook_secret() {
        for empty_secret in ["", "   "] {
            let err = MoyasarClient::builder("sk_test_123")
                .webhook_secret(empty_secret)
                .build()
                .expect_err("empty webhook secret must fail build");
            assert!(matches!(err, MoyasarError::InvalidWebhookSecret));
        }
    }

    #[tokio::test]
    async fn tc_1_1_0_01_spawns_10k_concurrent_clones_with_arc_integrity() {
        let client = MoyasarClient::new("sk_test_concurrent_10k").expect("client builds");
        assert_eq!(Arc::strong_count(&client.0), 1);

        let mut handles = Vec::with_capacity(10_000);
        for _ in 0..10_000 {
            let cloned = client.clone();
            handles.push(tokio::spawn(async move {
                assert_eq!(cloned.config().base_url, DEFAULT_BASE_URL);
                cloned
            }));
        }

        let mut held_clones = Vec::with_capacity(10_000);
        for handle in handles {
            held_clones.push(handle.await.expect("task completes cleanly"));
        }

        assert_eq!(Arc::strong_count(&client.0), 10_001);
        drop(held_clones);
        assert_eq!(Arc::strong_count(&client.0), 1);
    }
}
