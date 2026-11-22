//! Configuration builder and credential-redacted `Debug` formatting for the Moyasar adapter.

use core::fmt;

use secrecy::SecretString;
use some_payment_core::PaymentError;

use crate::alignment::key_scope::{MOYASAR_API_BASE_URL, detect_key_mode};

/// Configuration for [`MoyasarProvider`](super::moyasar_provider::MoyasarProvider).
///
/// Validates API key prefixes via [`detect_key_mode`], defaults `base_url` to
/// [`MOYASAR_API_BASE_URL`], denies raw card PAN submission by default (`allow_raw_cards = false`),
/// and redacts `api_key` and `webhook_secret` as `"[REDACTED]"` in [`fmt::Debug`] output.
///
/// # Examples
///
/// ```
/// use some_moyasar::adapter::moyasar_provider::MoyasarProviderConfig;
///
/// let cfg = MoyasarProviderConfig::new("sk_test_example123")
///     .expect("valid key")
///     .with_webhook_secret("wh_secret_123")
///     .with_base_url("http://127.0.0.1:8080");
/// let dbg = format!("{cfg:?}");
/// assert!(dbg.contains("[REDACTED]"));
/// assert!(!dbg.contains("sk_test_example123"));
/// assert!(!dbg.contains("wh_secret_123"));
/// ```
#[derive(Clone)]
pub struct MoyasarProviderConfig {
    pub(super) api_key: SecretString,
    pub(super) webhook_secret: Option<SecretString>,
    pub(super) base_url: String,
    pub(super) http_client: reqwest::Client,
    pub(super) allow_raw_cards: bool,
}

impl MoyasarProviderConfig {
    /// Creates a new configuration after validating `api_key` with [`detect_key_mode`].
    ///
    /// # Errors
    ///
    /// Returns [`PaymentError::ConfigurationError`] when `api_key` does not have a recognized
    /// Moyasar prefix (`sk_test_`, `sk_live_`, `pk_test_`, `pk_live_`) with a non-empty suffix.
    ///
    /// # Examples
    ///
    /// ```
    /// use some_moyasar::adapter::moyasar_provider::MoyasarProviderConfig;
    ///
    /// assert!(MoyasarProviderConfig::new("sk_test_abc123").is_ok());
    /// assert!(MoyasarProviderConfig::new("invalid_key").is_err());
    /// ```
    pub fn new(api_key: impl Into<String>) -> Result<Self, PaymentError> {
        let raw_key = api_key.into();
        detect_key_mode(&raw_key)
            .map_err(|_| PaymentError::configuration("invalid Moyasar API key prefix"))?;
        Ok(Self {
            api_key: SecretString::from(raw_key),
            webhook_secret: None,
            base_url: MOYASAR_API_BASE_URL.trim_end_matches('/').to_owned(),
            http_client: reqwest::Client::new(),
            allow_raw_cards: false,
        })
    }

    /// Configures the shared webhook secret used for constant-time `secret_token` verification.
    #[must_use]
    pub fn with_webhook_secret(mut self, secret: impl Into<String>) -> Self {
        self.webhook_secret = Some(SecretString::from(secret.into()));
        self
    }

    /// Overrides the Moyasar API base URL (for example, pointing to an offline `wiremock` server).
    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into().trim_end_matches('/').to_owned();
        self
    }

    /// Injects a shared, caller-configured [`reqwest::Client`].
    #[must_use]
    pub fn with_http_client(mut self, http_client: reqwest::Client) -> Self {
        self.http_client = http_client;
        self
    }

    /// Explicitly opts in or out of raw `creditcard` PAN submission (defaults to `false`).
    #[must_use]
    pub fn with_allow_raw_cards(mut self, allow_raw_cards: bool) -> Self {
        self.allow_raw_cards = allow_raw_cards;
        self
    }
}

impl fmt::Debug for MoyasarProviderConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MoyasarProviderConfig")
            .field("api_key", &"[REDACTED]")
            .field(
                "webhook_secret",
                &self.webhook_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .field("base_url", &self.base_url)
            .field("http_client", &"<reqwest::Client>")
            .field("allow_raw_cards", &self.allow_raw_cards)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use some_payment_core::PaymentError;

    use crate::alignment::key_scope::MOYASAR_API_BASE_URL;

    use super::MoyasarProviderConfig;

    #[test]
    fn new_validates_key_prefix_and_sets_safe_defaults() {
        for key in [
            "sk_test_cfg_1",
            "sk_live_cfg_2",
            "pk_test_cfg_3",
            "pk_live_cfg_4",
        ] {
            let cfg = MoyasarProviderConfig::new(key).expect("valid prefix");
            assert_eq!(cfg.base_url, MOYASAR_API_BASE_URL.trim_end_matches('/'));
            assert!(!cfg.allow_raw_cards);
            assert!(cfg.webhook_secret.is_none());
        }
        let err = MoyasarProviderConfig::new("bad_prefix_123").unwrap_err();
        assert!(matches!(err, PaymentError::ConfigurationError { .. }));
    }

    #[test]
    fn builders_trim_base_url_and_override_optional_fields() {
        let cfg = MoyasarProviderConfig::new("sk_test_builder_1")
            .expect("valid key")
            .with_base_url("http://127.0.0.1:9090///")
            .with_webhook_secret("wh_secret_builder")
            .with_http_client(reqwest::Client::new())
            .with_allow_raw_cards(true);
        assert_eq!(cfg.base_url, "http://127.0.0.1:9090");
        assert!(cfg.webhook_secret.is_some());
        assert!(cfg.allow_raw_cards);
    }

    #[test]
    fn debug_output_redacts_api_key_and_webhook_secret() {
        let cfg = MoyasarProviderConfig::new("sk_live_cfg_canary_999")
            .expect("valid key")
            .with_webhook_secret("wh_cfg_canary_888");
        let rendered = format!("{cfg:?}");
        assert!(rendered.contains("[REDACTED]"));
        assert!(!rendered.contains("sk_live_cfg_canary_999"));
        assert!(!rendered.contains("wh_cfg_canary_888"));
    }
}
