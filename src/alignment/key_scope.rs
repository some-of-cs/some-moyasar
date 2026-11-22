//! Key-prefix mode detection, local scope guards, and injectable HTTP client (US-9.1.0 Unit 1).
//!
//! Provides offline, zero-network pre-flight checks for Moyasar API key prefixes (`sk_test_`,
//! `sk_live_`, `pk_test_`, `pk_live_`), fail-closed guards blocking publishable keys on
//! secret-only operations, and an [`InjectableHttpConfig`] allowing callers and orchestrators to
//! share a pre-configured [`reqwest::Client`] or override the base URL for offline `wiremock`
//! tests.

use core::fmt;

use secrecy::SecretString;

use crate::kernel::error::MoyasarError;

/// Canonical Moyasar REST API v1 base URL.
pub const MOYASAR_API_BASE_URL: &str = "https://api.moyasar.com/v1";

/// Execution environment mode derived from a Moyasar API key prefix.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::key_scope::KeyMode;
///
/// assert!(KeyMode::Live.is_live());
/// assert!(!KeyMode::Live.is_test());
/// assert!(KeyMode::Test.is_test());
/// assert!(!KeyMode::Test.is_live());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyMode {
    /// Sandbox/test environment (`sk_test_` or `pk_test_`).
    Test,
    /// Production/live environment (`sk_live_` or `pk_live_`).
    Live,
}

impl KeyMode {
    /// Returns `true` when this mode represents the live production environment.
    #[must_use]
    pub const fn is_live(self) -> bool {
        matches!(self, Self::Live)
    }

    /// Returns `true` when this mode represents the test sandbox environment.
    #[must_use]
    pub const fn is_test(self) -> bool {
        matches!(self, Self::Test)
    }
}

/// Privilege tier derived from a Moyasar API key prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyKind {
    /// Backend-only secret key (`sk_test_...` or `sk_live_...`).
    Secret,
    /// Client-side publishable key (`pk_test_...` or `pk_live_...`).
    Publishable,
}

/// Combined privilege tier and environment mode for a validated Moyasar API key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyClassification {
    /// Whether the key is a secret (`sk_`) or publishable (`pk_`) key.
    pub kind: KeyKind,
    /// Whether the key targets the test or live environment.
    pub mode: KeyMode,
}

const KEY_PREFIXES: &[(&str, KeyKind, KeyMode)] = &[
    ("sk_test_", KeyKind::Secret, KeyMode::Test),
    ("sk_live_", KeyKind::Secret, KeyMode::Live),
    ("pk_test_", KeyKind::Publishable, KeyMode::Test),
    ("pk_live_", KeyKind::Publishable, KeyMode::Live),
];

/// Classifies a raw Moyasar API key by its prefix (`sk_test_`, `sk_live_`, `pk_test_`,
/// `pk_live_`).
///
/// Rejects keys with an unknown prefix or an empty suffix after the prefix, returning
/// [`MoyasarError::InvalidKey`] without including any raw key material in the error.
///
/// # Errors
///
/// Returns [`MoyasarError::InvalidKey`] if `raw_key` does not start with a recognized prefix or
/// has an empty suffix after the prefix.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::key_scope::{KeyKind, KeyMode, detect_key_mode};
///
/// let cls = detect_key_mode("sk_test_example123").unwrap();
/// assert_eq!(cls.kind, KeyKind::Secret);
/// assert_eq!(cls.mode, KeyMode::Test);
/// assert!(detect_key_mode("sk_test_").is_err());
/// ```
pub fn detect_key_mode(raw_key: &str) -> Result<KeyClassification, MoyasarError> {
    KEY_PREFIXES
        .iter()
        .find_map(|&(prefix, kind, mode)| {
            raw_key
                .strip_prefix(prefix)
                .filter(|suffix| !suffix.is_empty())
                .map(|_| KeyClassification { kind, mode })
        })
        .ok_or(MoyasarError::InvalidKey)
}

/// Moyasar API operations that strictly require a backend secret key (`sk_test_` or `sk_live_`).
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::key_scope::SecretOnlyOperation;
///
/// assert_eq!(SecretOnlyOperation::Capture.as_str(), "capture");
/// assert_eq!(
///     SecretOnlyOperation::ResendWebhookVerification.as_str(),
///     "resend_webhook_verification"
/// );
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SecretOnlyOperation {
    /// Capture a previously authorized payment.
    Capture,
    /// Void an authorized payment before capture.
    Void,
    /// Refund a paid or captured payment.
    Refund,
    /// Create a hosted payment invoice.
    CreateInvoice,
    /// Create a server-side payment intent.
    CreatePaymentIntent,
    /// Retrieve a single payment record.
    RetrievePayment,
    /// List payment records.
    ListPayments,
    /// Verify an incoming webhook event.
    VerifyWebhook,
    /// Resend a webhook verification attempt.
    ResendWebhookVerification,
}

const SECRET_ONLY_OPERATIONS: &[(SecretOnlyOperation, &str)] = &[
    (SecretOnlyOperation::Capture, "capture"),
    (SecretOnlyOperation::Void, "void"),
    (SecretOnlyOperation::Refund, "refund"),
    (SecretOnlyOperation::CreateInvoice, "create_invoice"),
    (
        SecretOnlyOperation::CreatePaymentIntent,
        "create_payment_intent",
    ),
    (SecretOnlyOperation::RetrievePayment, "retrieve_payment"),
    (SecretOnlyOperation::ListPayments, "list_payments"),
    (SecretOnlyOperation::VerifyWebhook, "verify_webhook"),
    (
        SecretOnlyOperation::ResendWebhookVerification,
        "resend_webhook_verification",
    ),
];

impl SecretOnlyOperation {
    /// Returns the canonical `snake_case` identifier for this secret-only operation.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        SECRET_ONLY_OPERATIONS
            .iter()
            .find_map(|&(op, name)| (op == self).then_some(name))
            .unwrap_or("unknown")
    }
}

/// Locally verifies that `raw_key` is a valid secret key (`sk_test_` or `sk_live_`) before
/// invoking a [`SecretOnlyOperation`].
///
/// Fails closed with zero network I/O when a publishable key (`pk_test_` / `pk_live_`) or an
/// invalid key is passed, without leaking raw key material in the returned [`MoyasarError`].
///
/// # Errors
///
/// - Returns [`MoyasarError::InvalidKey`] if `raw_key` has an unknown prefix or empty suffix.
/// - Returns [`MoyasarError::KeyTypeMismatch`] if `raw_key` is a publishable key (`pk_`).
///
/// # Examples
///
/// ```
/// use some_moyasar::MoyasarError;
/// use some_moyasar::alignment::key_scope::{
///     KeyMode, SecretOnlyOperation, require_secret_key,
/// };
///
/// let mode = require_secret_key("sk_live_abc123", SecretOnlyOperation::Refund).unwrap();
/// assert_eq!(mode, KeyMode::Live);
///
/// let err = require_secret_key("pk_live_abc123", SecretOnlyOperation::Refund).unwrap_err();
/// assert!(matches!(err, MoyasarError::KeyTypeMismatch));
/// ```
pub fn require_secret_key(raw_key: &str, op: SecretOnlyOperation) -> Result<KeyMode, MoyasarError> {
    let _ = op.as_str();
    let classification = detect_key_mode(raw_key)?;
    if classification.kind != KeyKind::Secret {
        return Err(MoyasarError::KeyTypeMismatch);
    }
    Ok(classification.mode)
}

/// HTTP transport configuration supporting caller-injected [`reqwest::Client`] instances and
/// custom base URLs for offline `wiremock` tests.
///
/// Any attached API key is stored in a [`SecretString`] and redacted in [`fmt::Debug`] output.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::key_scope::{InjectableHttpConfig, MOYASAR_API_BASE_URL};
///
/// let shared = reqwest::Client::new();
/// let cfg = InjectableHttpConfig::with_http_client(shared);
/// assert_eq!(cfg.base_url(), MOYASAR_API_BASE_URL);
///
/// let mock_cfg = cfg.with_base_url("http://127.0.0.1:8080");
/// assert_eq!(mock_cfg.base_url(), "http://127.0.0.1:8080");
/// ```
#[derive(Clone)]
pub struct InjectableHttpConfig {
    base_url: String,
    http_client: reqwest::Client,
    api_key: Option<SecretString>,
}

impl InjectableHttpConfig {
    /// Creates a new configuration using [`MOYASAR_API_BASE_URL`] and a default
    /// [`reqwest::Client`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_http_client(reqwest::Client::new())
    }

    /// Creates a configuration backed by an externally constructed, shared [`reqwest::Client`],
    /// defaulting `base_url` to [`MOYASAR_API_BASE_URL`].
    #[must_use]
    pub fn with_http_client(http_client: reqwest::Client) -> Self {
        Self {
            base_url: MOYASAR_API_BASE_URL.to_owned(),
            http_client,
            api_key: None,
        }
    }

    /// Overrides the target API base URL (for example, with a `wiremock` mock server URI).
    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into().trim_end_matches('/').to_owned();
        self
    }

    /// Attaches an optional API key that is zeroized on drop and redacted in [`fmt::Debug`].
    #[must_use]
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(SecretString::from(api_key.into()));
        self
    }

    /// Returns the configured API base URL (defaults to [`MOYASAR_API_BASE_URL`]).
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Returns a reference to the underlying [`reqwest::Client`].
    #[must_use]
    pub fn http_client(&self) -> &reqwest::Client {
        &self.http_client
    }
}

impl Default for InjectableHttpConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for InjectableHttpConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InjectableHttpConfig")
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .field("http_client", &"<reqwest::Client>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_SECRET_OPS: &[SecretOnlyOperation] = &[
        SecretOnlyOperation::Capture,
        SecretOnlyOperation::Void,
        SecretOnlyOperation::Refund,
        SecretOnlyOperation::CreateInvoice,
        SecretOnlyOperation::CreatePaymentIntent,
        SecretOnlyOperation::RetrievePayment,
        SecretOnlyOperation::ListPayments,
        SecretOnlyOperation::VerifyWebhook,
        SecretOnlyOperation::ResendWebhookVerification,
    ];

    #[test]
    fn tc_9_1_1_detects_all_valid_key_prefixes_and_modes() {
        let sk_test = detect_key_mode("sk_test_abc123").expect("valid sk_test_");
        assert_eq!(sk_test.kind, KeyKind::Secret);
        assert_eq!(sk_test.mode, KeyMode::Test);
        assert!(sk_test.mode.is_test());
        assert!(!sk_test.mode.is_live());

        let sk_live = detect_key_mode("sk_live_abc123").expect("valid sk_live_");
        assert_eq!(sk_live.kind, KeyKind::Secret);
        assert_eq!(sk_live.mode, KeyMode::Live);
        assert!(sk_live.mode.is_live());
        assert!(!sk_live.mode.is_test());

        let pk_test = detect_key_mode("pk_test_xyz789").expect("valid pk_test_");
        assert_eq!(pk_test.kind, KeyKind::Publishable);
        assert_eq!(pk_test.mode, KeyMode::Test);

        let pk_live = detect_key_mode("pk_live_xyz789").expect("valid pk_live_");
        assert_eq!(pk_live.kind, KeyKind::Publishable);
        assert_eq!(pk_live.mode, KeyMode::Live);
    }

    #[test]
    fn tc_9_1_1_rejects_empty_suffix_and_unknown_prefixes_without_leaking_key() {
        let invalid_keys = [
            "sk_test_",
            "sk_live_",
            "pk_test_",
            "pk_live_",
            "",
            "rk_test_secret_material_999",
            "SK_TEST_uppercase_secret_999",
            " sk_test_leading_space_999",
        ];

        for raw in invalid_keys {
            let err = detect_key_mode(raw).expect_err("invalid key must be rejected");
            assert!(matches!(err, MoyasarError::InvalidKey));
            let formatted = format!("{err} | {err:?}");
            assert!(!formatted.contains("secret_material_999"));
            assert!(!formatted.contains("uppercase_secret_999"));
            assert!(!formatted.contains("leading_space_999"));
        }
    }

    #[test]
    fn tc_9_1_1_blocks_publishable_keys_for_every_secret_only_operation() {
        for &op in ALL_SECRET_OPS {
            assert!(!op.as_str().is_empty());
            assert_ne!(op.as_str(), "unknown");

            let pk_test_err = require_secret_key("pk_test_sensitive_pub_token", op)
                .expect_err("pk_test_ must fail closed");
            assert!(matches!(pk_test_err, MoyasarError::KeyTypeMismatch));
            let pk_test_fmt = format!("{pk_test_err} | {pk_test_err:?}");
            assert!(!pk_test_fmt.contains("pk_test_sensitive_pub_token"));

            let pk_live_err = require_secret_key("pk_live_sensitive_live_token", op)
                .expect_err("pk_live_ must fail closed");
            assert!(matches!(pk_live_err, MoyasarError::KeyTypeMismatch));
            let pk_live_fmt = format!("{pk_live_err} | {pk_live_err:?}");
            assert!(!pk_live_fmt.contains("pk_live_sensitive_live_token"));

            let invalid_err =
                require_secret_key("sk_test_", op).expect_err("empty suffix must fail closed");
            assert!(matches!(invalid_err, MoyasarError::InvalidKey));

            assert_eq!(
                require_secret_key("sk_test_valid_secret", op).expect("sk_test_ allowed"),
                KeyMode::Test
            );
            assert_eq!(
                require_secret_key("sk_live_valid_secret", op).expect("sk_live_ allowed"),
                KeyMode::Live
            );
        }
    }

    #[test]
    fn tc_9_1_1_supports_injectable_reqwest_client_and_redacts_debug() {
        let custom_client = reqwest::Client::builder()
            .user_agent("some-payment-orchestrator/test")
            .build()
            .expect("build custom reqwest client");

        let default_cfg = InjectableHttpConfig::default();
        assert_eq!(default_cfg.base_url(), MOYASAR_API_BASE_URL);

        let injected = InjectableHttpConfig::with_http_client(custom_client)
            .with_base_url("http://127.0.0.1:45678/")
            .with_api_key("sk_live_super_secret_do_not_log");
        assert_eq!(injected.base_url(), "http://127.0.0.1:45678");
        let _client_ref: &reqwest::Client = injected.http_client();

        let debug_output = format!("{injected:?}");
        assert!(debug_output.contains("[REDACTED]"));
        assert!(!debug_output.contains("sk_live_super_secret_do_not_log"));
        assert!(!debug_output.contains("super_secret"));
    }
}
