//! Optional `some-payment-core::PaymentProvider` implementation for Moyasar (US-9.2.0 Unit 3).
//!
//! Bridges `some-moyasar`'s offline alignment guards, pre-flight validators, error classifier,
//! and constant-time webhook scrubber into the cross-gateway
//! [`some_payment_core::PaymentProvider`] SPI.

use core::fmt;

use async_trait::async_trait;
use reqwest::Method;
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Map, Value, json};
use some_payment_core::{
    CaptureMode, CaptureRequest, CheckoutSessionRequest, CreatePaymentRequest, Money, NextAction,
    NormalizedWebhookEvent, PaymentCapabilities, PaymentError, PaymentProvider, PaymentResult,
    PaymentStatus, ProviderId, ProviderReference, RawWebhookRequest, RefundRequest, RefundResult,
    RefundStatus, VerificationChallenge, VoidRequest,
};

use crate::alignment::checkout_intents::{InvoiceCheckoutInput, map_invoice_checkout};
use crate::alignment::idempotency_errors::{
    MoyasarErrorClass, map_moyasar_error, validate_given_id,
};
use crate::alignment::key_scope::{
    MOYASAR_API_BASE_URL, SecretOnlyOperation, detect_key_mode, require_secret_key,
};
use crate::alignment::pan_and_status::{
    MappedNextAction, MappedPaymentStatus, MoyasarPaymentSnapshot, RawCardPolicy, guard_raw_pan,
    map_moyasar_status,
};
use crate::alignment::preflight_rules::{
    MoyasarPreflightInput, SplitCheckItem, build_mutation_body, validate_metadata_limits,
    validate_moyasar_preflight, validate_partial_amount, validate_uuid_id,
};
use crate::alignment::webhook_handshake::{
    NormalizedMoyasarEvent, extract_verification_challenge, normalize_moyasar_event,
};
use crate::alignment::webhook_token_verifier::verify_and_scrub_moyasar_webhook;
use crate::kernel::MoyasarError;

const MOYASAR_PROVIDER_SLUG: &str = "moyasar";

const MAPPED_STATUS_TABLE: &[(MappedPaymentStatus, PaymentStatus)] = &[
    (MappedPaymentStatus::Initiated, PaymentStatus::Initiated),
    (
        MappedPaymentStatus::RequiresAction,
        PaymentStatus::RequiresAction,
    ),
    (MappedPaymentStatus::Authorized, PaymentStatus::Authorized),
    (MappedPaymentStatus::Captured, PaymentStatus::Captured),
    (MappedPaymentStatus::Paid, PaymentStatus::Paid),
    (MappedPaymentStatus::Refunded, PaymentStatus::Refunded),
    (MappedPaymentStatus::Voided, PaymentStatus::Voided),
    (MappedPaymentStatus::Failed, PaymentStatus::Failed),
    (
        MappedPaymentStatus::SetupCompleted,
        PaymentStatus::SetupCompleted,
    ),
];

const WEBHOOK_STATUS_TABLE: &[(NormalizedMoyasarEvent, PaymentStatus)] = &[
    (NormalizedMoyasarEvent::PaymentPaid, PaymentStatus::Paid),
    (NormalizedMoyasarEvent::PaymentFailed, PaymentStatus::Failed),
    (
        NormalizedMoyasarEvent::PaymentAuthorized,
        PaymentStatus::Authorized,
    ),
    (
        NormalizedMoyasarEvent::PaymentCaptured,
        PaymentStatus::Captured,
    ),
    (
        NormalizedMoyasarEvent::PaymentRefunded,
        PaymentStatus::Refunded,
    ),
    (NormalizedMoyasarEvent::PaymentVoided, PaymentStatus::Voided),
    (
        NormalizedMoyasarEvent::PaymentAbandoned,
        PaymentStatus::Canceled,
    ),
    (
        NormalizedMoyasarEvent::PaymentVerified,
        PaymentStatus::SetupCompleted,
    ),
];

/// Configuration for [`MoyasarProvider`].
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
    api_key: SecretString,
    webhook_secret: Option<SecretString>,
    base_url: String,
    http_client: reqwest::Client,
    allow_raw_cards: bool,
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
            base_url: MOYASAR_API_BASE_URL.to_owned(),
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

/// Optional [`PaymentProvider`] adapter backed by Moyasar REST API v1 and `some-moyasar`'s
/// offline alignment guards.
///
/// Environment mode (`livemode`) is strictly derived from the validated API key prefix via
/// [`detect_key_mode`] rather than a separate boolean sandbox flag. All credentials are redacted
/// in [`fmt::Debug`] output.
///
/// # Examples
///
/// ```
/// use some_moyasar::adapter::moyasar_provider::MoyasarProvider;
/// use some_payment_core::{PaymentCapabilities, PaymentProvider};
///
/// let provider = MoyasarProvider::from_api_key("sk_test_example123").expect("valid key");
/// assert_eq!(provider.id().as_str(), "moyasar");
/// assert!(!provider.livemode());
/// assert!(
///     provider
///         .capabilities()
///         .contains(PaymentCapabilities::NATIVE_CREATE_IDEMPOTENCY)
/// );
/// assert!(
///     !provider
///         .capabilities()
///         .contains(PaymentCapabilities::NATIVE_MUTATION_IDEMPOTENCY)
/// );
/// ```
#[derive(Clone)]
pub struct MoyasarProvider {
    provider_id: ProviderId,
    api_key: SecretString,
    webhook_secret: Option<SecretString>,
    base_url: String,
    http_client: reqwest::Client,
    livemode: bool,
    allow_raw_cards: bool,
}

impl MoyasarProvider {
    /// Constructs a [`MoyasarProvider`] from a validated [`MoyasarProviderConfig`].
    ///
    /// # Errors
    ///
    /// Returns [`PaymentError::ConfigurationError`] if the configured API key fails prefix
    /// validation via [`detect_key_mode`].
    pub fn new(config: MoyasarProviderConfig) -> Result<Self, PaymentError> {
        let classification = detect_key_mode(config.api_key.expose_secret())
            .map_err(|_| PaymentError::configuration("invalid Moyasar API key prefix"))?;
        Ok(Self {
            provider_id: ProviderId::new(MOYASAR_PROVIDER_SLUG),
            api_key: config.api_key,
            webhook_secret: config.webhook_secret,
            base_url: config.base_url,
            http_client: config.http_client,
            livemode: classification.mode.is_live(),
            allow_raw_cards: config.allow_raw_cards,
        })
    }

    /// Convenience constructor building a [`MoyasarProvider`] with default configuration from a
    /// raw Moyasar API key.
    ///
    /// # Errors
    ///
    /// Returns [`PaymentError::ConfigurationError`] if `api_key` fails [`detect_key_mode`].
    pub fn from_api_key(api_key: impl Into<String>) -> Result<Self, PaymentError> {
        MoyasarProviderConfig::new(api_key).and_then(Self::new)
    }

    /// Returns `true` when operating in Moyasar's live environment, strictly derived from the
    /// API key prefix (`sk_live_` / `pk_live_`).
    #[must_use]
    pub fn livemode(&self) -> bool {
        self.livemode
    }

    /// Returns the configured Moyasar API base URL.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn map_local_error(&self, err: MoyasarError) -> PaymentError {
        match err {
            MoyasarError::InvalidKey | MoyasarError::KeyTypeMismatch => {
                PaymentError::AuthenticationError {
                    provider: self.provider_id.clone(),
                    message: err.to_string(),
                }
            }
            MoyasarError::InvalidWebhookSecret => PaymentError::WebhookVerificationFailed {
                provider: self.provider_id.clone(),
                reason: err.to_string(),
            },
            other => PaymentError::validation(other.to_string()),
        }
    }

    fn require_secret_and_uuid(
        &self,
        op: SecretOnlyOperation,
        reference: &ProviderReference,
    ) -> Result<uuid::Uuid, PaymentError> {
        require_secret_key(self.api_key.expose_secret(), op)
            .map_err(|err| self.map_local_error(err))?;
        let raw_id = reference
            .payment_id()
            .ok_or_else(|| PaymentError::validation("ProviderReference requires payment_id"))?;
        validate_uuid_id(raw_id, "payment_id").map_err(|err| self.map_local_error(err))
    }

    async fn send_json(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, PaymentError> {
        let mut builder = self
            .http_client
            .request(method, format!("{}{path}", self.base_url))
            .basic_auth(self.api_key.expose_secret(), Some(""));
        if let Some(payload) = body {
            builder = builder.json(payload);
        }
        let resp = builder.send().await.map_err(|err| {
            let msg = if err.is_timeout() {
                "Moyasar request timed out"
            } else {
                "Moyasar transport failure"
            };
            PaymentError::ProviderUnavailable {
                provider: self.provider_id.clone(),
                http_status: None,
                indeterminate: true,
                message: msg.to_owned(),
            }
        })?;
        let status = resp.status();
        let retry_after = resp
            .headers()
            .get("retry-after")
            .and_then(|val| val.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok());
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(self.classify_http_error(status.as_u16(), &text, retry_after));
        }
        serde_json::from_str(&text).map_err(|_| PaymentError::ProviderError {
            provider: self.provider_id.clone(),
            http_status: Some(status.as_u16()),
            vendor_code: None,
            message: "invalid JSON response from Moyasar".to_owned(),
        })
    }

    fn classify_http_error(
        &self,
        status: u16,
        body: &str,
        retry_after: Option<u64>,
    ) -> PaymentError {
        let parsed = serde_json::from_str::<Value>(body).ok();
        let err_type = parsed.as_ref().and_then(|v| v.get("type")?.as_str());
        let err_code = parsed.as_ref().and_then(|v| v.get("code")?.as_str());
        let message = parsed
            .as_ref()
            .and_then(|v| v.get("message")?.as_str())
            .map_or_else(|| format!("Moyasar HTTP {status}"), str::to_owned);
        let classified = map_moyasar_error(Some(status), err_type, err_code);
        let provider = self.provider_id.clone();
        match classified.class {
            MoyasarErrorClass::ThreeDsFailed => PaymentError::ThreeDsFailed {
                provider,
                code: classified.vendor_code,
                message,
            },
            MoyasarErrorClass::AuthenticationError => {
                PaymentError::AuthenticationError { provider, message }
            }
            MoyasarErrorClass::ValidationError => PaymentError::ValidationError { message },
            MoyasarErrorClass::CardDeclined => PaymentError::CardDeclined {
                provider,
                decline_code: classified.vendor_code,
                advice_code: None,
                message,
            },
            MoyasarErrorClass::RateLimited => PaymentError::RateLimited {
                provider,
                retry_after_secs: retry_after,
            },
            MoyasarErrorClass::ProviderUnavailable => PaymentError::ProviderUnavailable {
                provider,
                http_status: classified.http_status,
                indeterminate: classified.indeterminate,
                message,
            },
            MoyasarErrorClass::ProviderError => PaymentError::ProviderError {
                provider,
                http_status: classified.http_status,
                vendor_code: classified.vendor_code,
                message,
            },
        }
    }

    fn parse_payment_result(
        &self,
        payload: &Value,
        order_id: Option<&str>,
    ) -> Result<PaymentResult, PaymentError> {
        let err = || PaymentError::ProviderError {
            provider: self.provider_id.clone(),
            http_status: None,
            vendor_code: None,
            message: "unexpected Moyasar payment response shape".to_owned(),
        };
        let id = payload.get("id").and_then(Value::as_str).ok_or_else(err)?;
        let status_raw = payload
            .get("status")
            .and_then(Value::as_str)
            .ok_or_else(err)?;
        let amount = payload
            .get("amount")
            .and_then(Value::as_u64)
            .ok_or_else(err)?;
        let currency = payload
            .get("currency")
            .and_then(Value::as_str)
            .ok_or_else(err)?;
        let src = payload.get("source");
        let bill_ref = src.and_then(|s| {
            s.get("bill_reference")
                .or_else(|| s.get("bill_number"))
                .or_else(|| s.get("reference_number"))
                .and_then(Value::as_str)
        });
        let snapshot = MoyasarPaymentSnapshot {
            status: status_raw,
            source_type: src.and_then(|s| s.get("type")?.as_str()),
            transaction_url: src.and_then(|s| s.get("transaction_url")?.as_str()),
            bill_reference: bill_ref,
        };
        let outcome = map_moyasar_status(&snapshot).map_err(|e| self.map_local_error(e))?;
        let mut ref_b = ProviderReference::builder(self.provider_id.clone()).payment_id(id);
        if let Some(order) = order_id {
            ref_b = ref_b.order_id(order);
        }
        let status = MAPPED_STATUS_TABLE
            .iter()
            .find(|(s, _)| *s == outcome.status)
            .map_or(PaymentStatus::Initiated, |(_, d)| *d);
        let next_action = outcome.next_action.map(|a| match a {
            MappedNextAction::Redirect { url } => NextAction::Redirect { url },
            MappedNextAction::OtpChallenge { submission_url } => {
                NextAction::OtpChallenge { submission_url }
            }
            MappedNextAction::OfflineBill {
                bill_reference,
                instructions_url,
            } => NextAction::OfflineBill {
                bill_reference,
                instructions_url,
            },
        });
        Ok(PaymentResult {
            reference: ref_b.build()?,
            status,
            money: Money::new(amount, currency)?,
            captured_amount: payload.get("captured").and_then(Value::as_u64),
            refunded_amount: payload.get("refunded").and_then(Value::as_u64),
            next_action,
            checkout_url: None,
            livemode: self.livemode,
        })
    }
}

impl fmt::Debug for MoyasarProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MoyasarProvider")
            .field("provider_id", &self.provider_id)
            .field("api_key", &"[REDACTED]")
            .field(
                "webhook_secret",
                &self.webhook_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .field("base_url", &self.base_url)
            .field("livemode", &self.livemode)
            .field("allow_raw_cards", &self.allow_raw_cards)
            .finish()
    }
}

#[async_trait]
impl PaymentProvider for MoyasarProvider {
    fn id(&self) -> &ProviderId {
        &self.provider_id
    }

    fn capabilities(&self) -> PaymentCapabilities {
        PaymentCapabilities::DIRECT_PAYMENT
            | PaymentCapabilities::EMBEDDED_INTENT
            | PaymentCapabilities::HOSTED_CHECKOUT
            | PaymentCapabilities::AUTH_CAPTURE
            | PaymentCapabilities::PARTIAL_CAPTURE
            | PaymentCapabilities::VOID_AUTHORIZED
            | PaymentCapabilities::REFUND
            | PaymentCapabilities::PARTIAL_REFUND
            | PaymentCapabilities::NATIVE_CREATE_IDEMPOTENCY
            | PaymentCapabilities::SAVED_CARDS
            | PaymentCapabilities::SPLITS
            | PaymentCapabilities::WEBHOOK_VERIFICATION_HANDSHAKE
    }

    async fn create_payment(
        &self,
        req: &CreatePaymentRequest,
    ) -> Result<PaymentResult, PaymentError> {
        req.validate()?;
        let source_type = req
            .metadata
            .get("source_type")
            .map(String::as_str)
            .or_else(|| {
                let tok = req.payment_token.as_deref().map(str::trim)?;
                if tok.eq_ignore_ascii_case("creditcard") || tok.eq_ignore_ascii_case("credit_card")
                {
                    Some("creditcard")
                } else {
                    Some("token")
                }
            });
        let policy = RawCardPolicy {
            allow_raw_cards: self.allow_raw_cards,
        };
        if let Some(st) = source_type {
            guard_raw_pan(st, policy).map_err(|err| self.map_local_error(err))?;
        }
        let splits: Vec<SplitCheckItem> = req
            .splits
            .iter()
            .map(|item| SplitCheckItem {
                recipient_id: item.recipient_id.clone(),
                amount: item.amount,
            })
            .collect();
        let preflight = MoyasarPreflightInput {
            payment_id: None,
            amount: req.money.amount(),
            manual_capture: matches!(req.capture_mode, CaptureMode::Manual),
            source_type,
            metadata: &req.metadata,
            splits: &splits,
        };
        validate_moyasar_preflight(&preflight).map_err(|err| self.map_local_error(err))?;
        let given_id = req
            .idempotency_key
            .as_deref()
            .map(|raw| validate_given_id(raw).map_err(|err| self.map_local_error(err)))
            .transpose()?;
        let mut map = Map::new();
        map.insert("amount".to_owned(), json!(req.money.amount()));
        map.insert("currency".to_owned(), json!(req.money.currency().as_str()));
        let desc = req.description.as_deref().unwrap_or(&req.order_id);
        map.insert("description".to_owned(), json!(desc));
        if let Some(url) = req.callback_url.as_deref() {
            map.insert("callback_url".to_owned(), json!(url));
        }
        if let Some(id) = given_id {
            map.insert("given_id".to_owned(), json!(id.to_string()));
        }
        if !req.metadata.is_empty() {
            map.insert("metadata".to_owned(), json!(&req.metadata));
        }
        let kind = source_type.unwrap_or("token");
        let mut src = Map::new();
        src.insert("type".to_owned(), json!(kind));
        if matches!(req.capture_mode, CaptureMode::Manual) {
            src.insert("manual".to_owned(), json!(true));
        }
        if let Some(tok) = req
            .payment_token
            .as_deref()
            .filter(|_| kind.eq_ignore_ascii_case("token"))
        {
            src.insert("token".to_owned(), json!(tok));
        }
        map.insert("source".to_owned(), Value::Object(src));
        if !req.splits.is_empty() {
            let items: Vec<Value> = req
                .splits
                .iter()
                .map(|s| {
                    json!({
                        "recipient_id": s.recipient_id,
                        "amount": s.amount,
                        "fee_source": s.fee_source,
                    })
                })
                .collect();
            map.insert("splits".to_owned(), Value::Array(items));
        }
        let body = Value::Object(map);
        let payload = self.send_json(Method::POST, "/payments", Some(&body)).await?;
        self.parse_payment_result(&payload, Some(&req.order_id))
    }

    async fn get_payment(
        &self,
        reference: &ProviderReference,
    ) -> Result<PaymentResult, PaymentError> {
        let uuid = self.require_secret_and_uuid(SecretOnlyOperation::RetrievePayment, reference)?;
        let payload = self
            .send_json(Method::GET, &format!("/payments/{uuid}"), None)
            .await?;
        self.parse_payment_result(&payload, reference.order_id())
    }

    async fn create_checkout(
        &self,
        req: &CheckoutSessionRequest,
    ) -> Result<PaymentResult, PaymentError> {
        require_secret_key(
            self.api_key.expose_secret(),
            SecretOnlyOperation::CreateInvoice,
        )
        .map_err(|err| self.map_local_error(err))?;
        req.validate()?;
        validate_metadata_limits(&req.metadata).map_err(|err| self.map_local_error(err))?;
        let mut map = Map::new();
        map.insert("amount".to_owned(), json!(req.money.amount()));
        map.insert("currency".to_owned(), json!(req.money.currency().as_str()));
        let desc = req.description.as_deref().unwrap_or(&req.order_id);
        map.insert("description".to_owned(), json!(desc));
        map.insert("success_url".to_owned(), json!(&req.success_url));
        if let Some(cancel) = req.cancel_url.as_deref() {
            map.insert("back_url".to_owned(), json!(cancel));
        }
        if !req.metadata.is_empty() {
            map.insert("metadata".to_owned(), json!(&req.metadata));
        }
        let body = Value::Object(map);
        let payload = self.send_json(Method::POST, "/invoices", Some(&body)).await?;
        let err = || PaymentError::ProviderError {
            provider: self.provider_id.clone(),
            http_status: None,
            vendor_code: None,
            message: "unexpected Moyasar invoice response shape".to_owned(),
        };
        let outcome = map_invoice_checkout(InvoiceCheckoutInput {
            invoice_id: payload.get("id").and_then(Value::as_str).ok_or_else(err)?,
            invoice_url: payload.get("url").and_then(Value::as_str),
            amount: payload
                .get("amount")
                .and_then(Value::as_u64)
                .ok_or_else(err)?,
            currency: payload
                .get("currency")
                .and_then(Value::as_str)
                .ok_or_else(err)?,
        })
        .map_err(|e| self.map_local_error(e))?;
        let reference = ProviderReference::builder(self.provider_id.clone())
            .checkout_session_id(&outcome.invoice_id)
            .order_id(&req.order_id)
            .build()?;
        Ok(PaymentResult {
            reference,
            status: PaymentStatus::RequiresAction,
            money: Money::new(outcome.amount, &outcome.currency)?,
            captured_amount: None,
            refunded_amount: None,
            next_action: Some(NextAction::Redirect {
                url: outcome.checkout_url.clone(),
            }),
            checkout_url: Some(outcome.checkout_url),
            livemode: self.livemode,
        })
    }

    async fn capture(&self, req: &CaptureRequest) -> Result<PaymentResult, PaymentError> {
        let uuid = self.require_secret_and_uuid(SecretOnlyOperation::Capture, &req.reference)?;
        req.validate()?;
        let partial = validate_partial_amount(req.amount.as_ref().map(Money::amount))
            .map_err(|err| self.map_local_error(err))?;
        let body = build_mutation_body(partial);
        let payload = self
            .send_json(
                Method::POST,
                &format!("/payments/{uuid}/capture"),
                body.as_ref(),
            )
            .await?;
        self.parse_payment_result(&payload, req.reference.order_id())
    }

    async fn void_payment(&self, req: &VoidRequest) -> Result<PaymentResult, PaymentError> {
        let uuid = self.require_secret_and_uuid(SecretOnlyOperation::Void, &req.reference)?;
        req.validate()?;
        let payload = self
            .send_json(Method::POST, &format!("/payments/{uuid}/void"), None)
            .await?;
        self.parse_payment_result(&payload, req.reference.order_id())
    }

    async fn refund(&self, req: &RefundRequest) -> Result<RefundResult, PaymentError> {
        let uuid = self.require_secret_and_uuid(SecretOnlyOperation::Refund, &req.reference)?;
        req.validate()?;
        let partial = validate_partial_amount(req.amount.as_ref().map(Money::amount))
            .map_err(|err| self.map_local_error(err))?;
        let body = build_mutation_body(partial);
        let payload = self
            .send_json(
                Method::POST,
                &format!("/payments/{uuid}/refund"),
                body.as_ref(),
            )
            .await?;
        let err = || PaymentError::ProviderError {
            provider: self.provider_id.clone(),
            http_status: None,
            vendor_code: None,
            message: "unexpected Moyasar refund response shape".to_owned(),
        };
        let id = payload.get("id").and_then(Value::as_str).ok_or_else(err)?;
        let status_raw = payload
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("refunded");
        let status = if status_raw.eq_ignore_ascii_case("failed") {
            RefundStatus::Failed
        } else {
            RefundStatus::Succeeded
        };
        let money = match req.amount.as_ref() {
            Some(p) => p.clone(),
            None => {
                let amt = payload
                    .get("refunded")
                    .and_then(Value::as_u64)
                    .filter(|&v| v > 0)
                    .or_else(|| payload.get("amount").and_then(Value::as_u64))
                    .ok_or_else(err)?;
                let cur = payload
                    .get("currency")
                    .and_then(Value::as_str)
                    .ok_or_else(err)?;
                Money::new(amt, cur)?
            }
        };
        Ok(RefundResult {
            refund_id: id.to_owned(),
            payment_reference: req.reference.clone(),
            status,
            money,
            reason_detail: None,
        })
    }

    async fn parse_and_verify_webhook(
        &self,
        raw: &RawWebhookRequest,
    ) -> Result<NormalizedWebhookEvent, PaymentError> {
        let secret = self
            .webhook_secret
            .as_ref()
            .ok_or_else(|| PaymentError::configuration("webhook_secret is not configured"))?;
        let verified = verify_and_scrub_moyasar_webhook(&raw.raw_body, secret.expose_secret())
            .map_err(|err| self.map_local_error(err))?;
        let normalized = normalize_moyasar_event(&verified.event_type);
        let status = normalized.and_then(|evt| {
            WEBHOOK_STATUS_TABLE
                .iter()
                .find(|(src, _)| *src == evt)
                .map(|(_, st)| *st)
        });
        let verification_challenge =
            if normalized == Some(NormalizedMoyasarEvent::WebhookVerification) {
                let challenge = extract_verification_challenge(&verified.raw_redacted)
                    .map_err(|err| self.map_local_error(err))?;
                Some(VerificationChallenge {
                    webhook_id: challenge.webhook_id.clone(),
                    verification_code: challenge.verification_code.clone(),
                })
            } else {
                None
            };
        let data = verified.raw_redacted.get("data");
        let reference = data.and_then(|d| d.get("id")?.as_str()).and_then(|pid| {
            ProviderReference::builder(self.provider_id.clone())
                .payment_id(pid)
                .build()
                .ok()
        });
        let money = data
            .and_then(|d| {
                d.get("amount")?
                    .as_u64()
                    .zip(d.get("currency")?.as_str())
            })
            .and_then(|(amt, cur)| Money::new(amt, cur).ok());
        Ok(NormalizedWebhookEvent {
            provider_id: self.provider_id.clone(),
            event_id: verified.event_id,
            event_type: verified.event_type,
            livemode: verified.livemode,
            requires_hydration: false,
            reference,
            status,
            money,
            verification_challenge,
            raw_redacted: verified.raw_redacted,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;
    use some_payment_core::{
        CaptureMode, CaptureRequest, CheckoutSessionRequest, CreatePaymentRequest, HttpMethod,
        Money, PaymentCapabilities, PaymentError, PaymentProvider, PaymentStatus, ProviderId,
        ProviderReference, RawWebhookRequest, RefundRequest, VoidRequest,
    };

    use super::{MoyasarProvider, MoyasarProviderConfig};

    const VALID_UUID: &str = "550e8400-e29b-41d4-a716-446655440000";

    #[test]
    fn provider_id_capabilities_livemode_and_debug_redaction() {
        let cfg = MoyasarProviderConfig::new("sk_live_canary_api_999")
            .expect("valid")
            .with_webhook_secret("wh_canary_secret_888");
        let cfg_dbg = format!("{cfg:?}");
        assert!(cfg_dbg.contains("[REDACTED]"));
        assert!(!cfg_dbg.contains("sk_live_canary_api_999"));
        assert!(!cfg_dbg.contains("wh_canary_secret_888"));

        let live = MoyasarProvider::new(cfg).expect("valid");
        let prov_dbg = format!("{live:?}");
        assert!(prov_dbg.contains("[REDACTED]"));
        assert!(!prov_dbg.contains("sk_live_canary_api_999"));
        assert!(!prov_dbg.contains("wh_canary_secret_888"));
        assert_eq!(live.id().as_str(), "moyasar");
        assert!(live.livemode());

        let caps = live.capabilities();
        assert!(caps.contains(PaymentCapabilities::DIRECT_PAYMENT));
        assert!(caps.contains(PaymentCapabilities::EMBEDDED_INTENT));
        assert!(caps.contains(PaymentCapabilities::HOSTED_CHECKOUT));
        assert!(caps.contains(PaymentCapabilities::AUTH_CAPTURE));
        assert!(caps.contains(PaymentCapabilities::PARTIAL_CAPTURE));
        assert!(caps.contains(PaymentCapabilities::VOID_AUTHORIZED));
        assert!(caps.contains(PaymentCapabilities::REFUND));
        assert!(caps.contains(PaymentCapabilities::PARTIAL_REFUND));
        assert!(caps.contains(PaymentCapabilities::NATIVE_CREATE_IDEMPOTENCY));
        assert!(caps.contains(PaymentCapabilities::SAVED_CARDS));
        assert!(caps.contains(PaymentCapabilities::SPLITS));
        assert!(caps.contains(PaymentCapabilities::WEBHOOK_VERIFICATION_HANDSHAKE));
        assert!(!caps.contains(PaymentCapabilities::NATIVE_MUTATION_IDEMPOTENCY));
        assert!(!caps.contains(PaymentCapabilities::VOID_SETTLED_WINDOW));

        let sk_test = MoyasarProvider::from_api_key("sk_test_abc123").expect("test");
        let pk_live = MoyasarProvider::from_api_key("pk_live_abc123").expect("live");
        let pk_test = MoyasarProvider::from_api_key("pk_test_abc123").expect("test");
        assert!(!sk_test.livemode());
        assert!(pk_live.livemode());
        assert!(!pk_test.livemode());
        assert!(MoyasarProvider::from_api_key("bad_prefix_123").is_err());
    }

    #[tokio::test]
    async fn local_fail_closed_guards_for_pk_and_raw_pan() {
        let pk_cfg = MoyasarProviderConfig::new("pk_test_local_guard")
            .expect("valid")
            .with_base_url("http://127.0.0.1:1");
        let pk_provider = MoyasarProvider::new(pk_cfg).expect("valid");
        let reference = ProviderReference::builder(ProviderId::new("moyasar"))
            .payment_id(VALID_UUID)
            .build()
            .expect("valid reference");

        assert!(matches!(
            pk_provider.get_payment(&reference).await.unwrap_err(),
            PaymentError::AuthenticationError { .. }
        ));
        let checkout_req = CheckoutSessionRequest {
            provider_id: None,
            money: Money::new(5_000, "SAR").expect("money"),
            order_id: "ORD-PK-1".to_owned(),
            success_url: "https://example.com/ok".to_owned(),
            cancel_url: None,
            description: None,
            customer_email: None,
            customer_phone: None,
            capture_mode: CaptureMode::Automatic,
            idempotency_key: None,
            metadata: BTreeMap::new(),
        };
        assert!(matches!(
            pk_provider.create_checkout(&checkout_req).await.unwrap_err(),
            PaymentError::AuthenticationError { .. }
        ));
        let cap_req = CaptureRequest {
            reference: reference.clone(),
            amount: None,
            idempotency_key: None,
        };
        assert!(matches!(
            pk_provider.capture(&cap_req).await.unwrap_err(),
            PaymentError::AuthenticationError { .. }
        ));
        let void_req = VoidRequest {
            reference: reference.clone(),
            reason: None,
            idempotency_key: None,
        };
        assert!(matches!(
            pk_provider.void_payment(&void_req).await.unwrap_err(),
            PaymentError::AuthenticationError { .. }
        ));
        let refund_req = RefundRequest {
            reference,
            amount: None,
            reason: None,
            idempotency_key: None,
        };
        assert!(matches!(
            pk_provider.refund(&refund_req).await.unwrap_err(),
            PaymentError::AuthenticationError { .. }
        ));

        let sk_cfg = MoyasarProviderConfig::new("sk_test_raw_pan_guard")
            .expect("valid")
            .with_base_url("http://127.0.0.1:1");
        let sk_provider = MoyasarProvider::new(sk_cfg).expect("valid");
        let mut req = CreatePaymentRequest::new(Money::new(10_000, "SAR").expect("money"), "ORD-1");
        req.metadata
            .insert("source_type".to_owned(), "creditcard".to_owned());
        assert!(matches!(
            sk_provider.create_payment(&req).await.unwrap_err(),
            PaymentError::ValidationError { .. }
        ));
    }

    #[tokio::test]
    async fn parse_and_verify_webhook_scrubs_secret_and_handles_typo_and_handshake() {
        let cfg = MoyasarProviderConfig::new("sk_test_wh_verify")
            .expect("valid")
            .with_webhook_secret("wh_secret_canary_777");
        let provider = MoyasarProvider::new(cfg).expect("valid");

        let faild_body = json!({
            "id": "evt_typo_1",
            "type": "payment_faild",
            "live": false,
            "secret_token": "wh_secret_canary_777",
            "data": { "id": VALID_UUID, "amount": 2500, "currency": "SAR" }
        })
        .to_string()
        .into_bytes();
        let raw_faild = RawWebhookRequest {
            raw_body: faild_body,
            headers: BTreeMap::new(),
            query_params: BTreeMap::new(),
            method: HttpMethod::Post,
        };
        let event = provider
            .parse_and_verify_webhook(&raw_faild)
            .await
            .expect("verified");
        assert_eq!(event.status, Some(PaymentStatus::Failed));
        assert!(!event.livemode);
        assert!(
            !event
                .raw_redacted
                .to_string()
                .contains("wh_secret_canary_777")
        );

        let handshake_body = json!({
            "id": "evt_hs_1",
            "type": "webhook_verification",
            "live": true,
            "secret_token": "wh_secret_canary_777",
            "data": { "webhook_id": "wh_live_42", "verification_code": "123456" }
        })
        .to_string()
        .into_bytes();
        let raw_hs = RawWebhookRequest {
            raw_body: handshake_body,
            headers: BTreeMap::new(),
            query_params: BTreeMap::new(),
            method: HttpMethod::Post,
        };
        let hs = provider
            .parse_and_verify_webhook(&raw_hs)
            .await
            .expect("handshake verified");
        assert!(hs.livemode);
        let challenge = hs.verification_challenge.expect("challenge present");
        assert_eq!(challenge.webhook_id, "wh_live_42");
        assert_eq!(challenge.verification_code, "123456");
        assert!(!hs.raw_redacted.to_string().contains("wh_secret_canary_777"));
    }
}
