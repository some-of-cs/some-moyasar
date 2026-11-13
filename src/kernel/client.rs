//! Thread-safe `MoyasarClient` backed by `Arc<ClientInner<K>>`.

use core::fmt;
use std::sync::Arc;
use std::time::Duration;

use crate::kernel::auth::{AuthKey, SecretKey};
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
}

impl Default for MoyasarClientConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_owned(),
            timeout: Duration::from_secs(30),
            max_network_retries: 2,
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

    /// Returns a reference to the client's authentication key.
    pub fn key(&self) -> &K {
        &self.0.key
    }
}

impl<K: AuthKey> MoyasarClientBuilder<K> {
    /// Overrides the API base URL (must start with `https://`).
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.config.base_url = url.into();
        self
    }

    /// Sets the per-request timeout.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    /// Sets the maximum retry count for transient network and 5xx failures.
    pub fn max_network_retries(mut self, retries: u32) -> Self {
        self.config.max_network_retries = retries;
        self
    }

    /// Validates configuration and builds the `MoyasarClient`.
    pub fn build(self) -> Result<MoyasarClient<K>, MoyasarError> {
        if !self.config.base_url.starts_with("https://") {
            return Err(MoyasarError::InvalidBaseUrl);
        }
        let headers = build_default_headers(&self.key)?;
        let http = reqwest::Client::builder()
            .timeout(self.config.timeout)
            .default_headers(headers)
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
    fn rejects_plaintext_http_base_url() {
        let key = SecretKey::new("sk_test_123").expect("valid key");
        let err = MoyasarClient::builder(key)
            .base_url("http://api.moyasar.com")
            .build()
            .expect_err("must reject http://");
        assert!(matches!(err, MoyasarError::InvalidBaseUrl));
    }

    #[test]
    fn arc_clone_shares_inner_and_redacts_debug() {
        let key = SecretKey::new("sk_test_secret_value").expect("valid key");
        let client = MoyasarClient::new(key).expect("client builds");
        let cloned = client.clone();
        assert_eq!(Arc::strong_count(&client.0), 2);
        assert!(!format!("{cloned:?}").contains("sk_test_secret_value"));
    }
}
