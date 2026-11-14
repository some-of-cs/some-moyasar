//! Thread-safe `MoyasarClient` backed by `Arc<ClientInner<K>>`.

use core::fmt;
use std::sync::Arc;
use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};

use crate::kernel::auth::{AuthKey, AuthMode, SecretKey};
use crate::kernel::error::MoyasarError;
use crate::kernel::transport::build_default_headers;

/// Default Moyasar API base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.moyasar.com";

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
    key: K,
    config: MoyasarClientConfig,
}

impl<K: AuthKey> MoyasarClient<K> {
    /// Creates a new `MoyasarClient` with default settings.
    pub fn new(key: K) -> Result<Self, MoyasarError> {
        Self::builder(key).build()
    }

    /// Starts a builder for customizing timeouts, retries, and base URL.
    pub fn builder(key: K) -> MoyasarClientBuilder<K> {
        MoyasarClientBuilder {
            key,
            config: MoyasarClientConfig::default(),
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
    if !has_clean_authority(&parsed) || !is_permitted_scheme_and_host(&parsed, mode) {
        return Err(MoyasarError::InvalidBaseUrl);
    }
    Ok(())
}

fn has_clean_authority(parsed: &reqwest::Url) -> bool {
    parsed.username().is_empty() && parsed.password().is_none() && parsed.host_str().is_some()
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

    /// Configures the shared webhook secret used by `verify_webhook`.
    #[must_use]
    pub fn webhook_secret(mut self, secret: impl Into<String>) -> Self {
        self.config.webhook_secret = Some(SecretString::from(secret.into()));
        self
    }

    /// Validates configuration and builds the `MoyasarClient`.
    pub fn build(mut self) -> Result<MoyasarClient<K>, MoyasarError> {
        let trimmed_url = self.config.base_url.trim_end_matches('/').to_owned();
        validate_base_url_for_mode(&trimmed_url, self.key.mode())?;
        self.config.base_url = trimmed_url;
        if self
            .config
            .webhook_secret
            .as_ref()
            .is_some_and(|s| s.expose_secret().trim().is_empty())
        {
            return Err(MoyasarError::InvalidWebhookSecret);
        }
        let headers = build_default_headers(&self.key)?;
        let http = reqwest::Client::builder()
            .timeout(self.config.timeout)
            .default_headers(headers)
            .https_only(self.key.mode() == AuthMode::Live)
            .build()?;

        Ok(MoyasarClient(Arc::new(ClientInner {
            key: self.key,
            http,
            config: self.config,
        })))
    }
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
            let key = SecretKey::new("sk_test_123").expect("valid key");
            let err = MoyasarClient::builder(key)
                .base_url(bad_url)
                .build()
                .expect_err("must reject invalid or userinfo base URL");
            assert!(matches!(err, MoyasarError::InvalidBaseUrl));
        }
    }

    #[test]
    fn rejects_loopback_http_when_key_is_live_mode() {
        let live_key = SecretKey::new("sk_live_123").expect("valid live key");
        let err = MoyasarClient::builder(live_key)
            .base_url("http://127.0.0.1:8080")
            .build()
            .expect_err("live key must reject loopback http");
        assert!(matches!(err, MoyasarError::InvalidBaseUrl));
    }

    #[test]
    fn arc_clone_shares_inner_and_redacts_debug() {
        let key = SecretKey::new("sk_test_secret_value").expect("valid key");
        let client = MoyasarClient::builder(key)
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
            let key = SecretKey::new("sk_test_123").expect("valid key");
            let err = MoyasarClient::builder(key)
                .webhook_secret(empty_secret)
                .build()
                .expect_err("empty webhook secret must fail build");
            assert!(matches!(err, MoyasarError::InvalidWebhookSecret));
        }
    }

    #[tokio::test]
    async fn tc_1_1_0_01_spawns_10k_concurrent_clones_with_arc_integrity() {
        let key = SecretKey::new("sk_test_concurrent_10k").expect("valid key");
        let client = MoyasarClient::new(key).expect("client builds");
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
