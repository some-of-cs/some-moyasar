//! Optional `some-payment-core::PaymentProvider` implementation for Moyasar (US-9.2.0 Unit 3).
//!
//! Bridges `some-moyasar`'s offline alignment guards, pre-flight validators, error classifier,
//! and constant-time webhook scrubber into the cross-gateway
//! [`some_payment_core::PaymentProvider`] SPI.

use core::fmt;

use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use some_payment_core::{
    CaptureMode, CaptureRequest, CheckoutSessionRequest, CreatePaymentRequest, Money, NextAction,
    NormalizedWebhookEvent, PaymentCapabilities, PaymentError, PaymentProvider, PaymentResult,
    PaymentStatus, ProviderId, ProviderReference, RawWebhookRequest, RefundRequest, RefundResult,
    RefundStatus, VerificationChallenge, VoidRequest,
};

use crate::alignment::checkout_intents::{InvoiceCheckoutInput, map_invoice_checkout};
use crate::alignment::idempotency_errors::{
    ClassifiedMoyasarError, MoyasarErrorClass, map_moyasar_error, validate_given_id,
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

    fn require_secret(&self, op: SecretOnlyOperation) -> Result<(), PaymentError> {
        require_secret_key(self.api_key.expose_secret(), op)
            .map(|_| ())
            .map_err(|err| map_local_error(&self.provider_id, err))
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    async fn send_get(&self, path: &str) -> Result<Value, PaymentError> {
        let resp = self
            .http_client
            .get(self.endpoint(path))
            .basic_auth(self.api_key.expose_secret(), Some(""))
            .send()
            .await
            .map_err(|err| map_transport_error(&self.provider_id, &err))?;
        parse_json_response(&self.provider_id, resp).await
    }

    async fn send_post(&self, path: &str, body: Option<&Value>) -> Result<Value, PaymentError> {
        let mut builder = self
            .http_client
            .post(self.endpoint(path))
            .basic_auth(self.api_key.expose_secret(), Some(""));
        if let Some(payload) = body {
            builder = builder.json(payload);
        }
        let resp = builder
            .send()
            .await
            .map_err(|err| map_transport_error(&self.provider_id, &err))?;
        parse_json_response(&self.provider_id, resp).await
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
        let source_type = resolve_source_type(req);
        validate_create_payment_guards(req, source_type, self.allow_raw_cards, &self.provider_id)?;
        let given_id = req
            .idempotency_key
            .as_deref()
            .map(|raw| validate_given_id(raw).map_err(|err| map_local_error(&self.provider_id, err)))
            .transpose()?;
        let body = build_create_payment_body(req, source_type, given_id);
        let payload = self.send_post("/payments", Some(&body)).await?;
        parse_payment_result(
            &self.provider_id,
            &payload,
            Some(&req.order_id),
            self.livemode,
        )
    }

    async fn get_payment(
        &self,
        reference: &ProviderReference,
    ) -> Result<PaymentResult, PaymentError> {
        self.require_secret(SecretOnlyOperation::RetrievePayment)?;
        let payment_uuid = extract_payment_uuid(reference, &self.provider_id)?;
        let payload = self.send_get(&format!("/payments/{payment_uuid}")).await?;
        parse_payment_result(
            &self.provider_id,
            &payload,
            reference.order_id(),
            self.livemode,
        )
    }

    async fn create_checkout(
        &self,
        req: &CheckoutSessionRequest,
    ) -> Result<PaymentResult, PaymentError> {
        self.require_secret(SecretOnlyOperation::CreateInvoice)?;
        req.validate()?;
        validate_metadata_limits(&req.metadata)
            .map_err(|err| map_local_error(&self.provider_id, err))?;
        let body = build_invoice_body(req);
        let payload = self.send_post("/invoices", Some(&body)).await?;
        parse_invoice_result(&self.provider_id, &payload, &req.order_id, self.livemode)
    }

    async fn capture(&self, req: &CaptureRequest) -> Result<PaymentResult, PaymentError> {
        self.require_secret(SecretOnlyOperation::Capture)?;
        req.validate()?;
        let payment_uuid = extract_payment_uuid(&req.reference, &self.provider_id)?;
        let partial = validate_partial_amount(req.amount.as_ref().map(Money::amount))
            .map_err(|err| map_local_error(&self.provider_id, err))?;
        let body = build_mutation_body(partial);
        let payload = self
            .send_post(&format!("/payments/{payment_uuid}/capture"), body.as_ref())
            .await?;
        parse_payment_result(
            &self.provider_id,
            &payload,
            req.reference.order_id(),
            self.livemode,
        )
    }

    async fn void_payment(&self, req: &VoidRequest) -> Result<PaymentResult, PaymentError> {
        self.require_secret(SecretOnlyOperation::Void)?;
        req.validate()?;
        let payment_uuid = extract_payment_uuid(&req.reference, &self.provider_id)?;
        let payload = self
            .send_post(&format!("/payments/{payment_uuid}/void"), None)
            .await?;
        parse_payment_result(
            &self.provider_id,
            &payload,
            req.reference.order_id(),
            self.livemode,
        )
    }

    async fn refund(&self, req: &RefundRequest) -> Result<RefundResult, PaymentError> {
        self.require_secret(SecretOnlyOperation::Refund)?;
        req.validate()?;
        let payment_uuid = extract_payment_uuid(&req.reference, &self.provider_id)?;
        let partial = validate_partial_amount(req.amount.as_ref().map(Money::amount))
            .map_err(|err| map_local_error(&self.provider_id, err))?;
        let body = build_mutation_body(partial);
        let payload = self
            .send_post(&format!("/payments/{payment_uuid}/refund"), body.as_ref())
            .await?;
        parse_refund_result(&self.provider_id, &payload, req)
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
            .map_err(|err| map_local_error(&self.provider_id, err))?;
        let normalized = normalize_moyasar_event(&verified.event_type);
        let status = normalized.and_then(map_webhook_event_status);
        let verification_challenge =
            extract_challenge_if_handshake(normalized, &verified.raw_redacted, &self.provider_id)?;
        let reference = extract_webhook_reference(&self.provider_id, &verified.raw_redacted);
        let money = extract_webhook_money(&verified.raw_redacted);
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

#[derive(Deserialize)]
struct RawMoyasarError {
    #[serde(rename = "type")]
    error_type: Option<String>,
    code: Option<String>,
    message: Option<String>,
}

#[derive(Deserialize)]
struct RawMoyasarSource {
    #[serde(rename = "type")]
    source_type: Option<String>,
    transaction_url: Option<String>,
    bill_reference: Option<String>,
    bill_number: Option<String>,
    reference_number: Option<String>,
}

#[derive(Deserialize)]
struct RawMoyasarPayment {
    id: String,
    status: String,
    amount: u64,
    currency: String,
    captured: Option<u64>,
    refunded: Option<u64>,
    source: Option<RawMoyasarSource>,
}

#[derive(Deserialize)]
struct RawMoyasarInvoice {
    id: String,
    url: Option<String>,
    amount: u64,
    currency: String,
}

fn resolve_source_type(req: &CreatePaymentRequest) -> Option<&str> {
    if let Some(explicit) = req.metadata.get("source_type").map(String::as_str) {
        return Some(explicit);
    }
    let token = req.payment_token.as_deref().map(str::trim)?;
    if token.eq_ignore_ascii_case("creditcard") || token.eq_ignore_ascii_case("credit_card") {
        return Some("creditcard");
    }
    Some("token")
}

fn validate_create_payment_guards(
    req: &CreatePaymentRequest,
    source_type: Option<&str>,
    allow_raw_cards: bool,
    provider_id: &ProviderId,
) -> Result<(), PaymentError> {
    let policy = RawCardPolicy { allow_raw_cards };
    if let Some(st) = source_type {
        guard_raw_pan(st, policy).map_err(|err| map_local_error(provider_id, err))?;
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
    validate_moyasar_preflight(&preflight).map_err(|err| map_local_error(provider_id, err))
}

fn build_create_payment_body(
    req: &CreatePaymentRequest,
    source_type: Option<&str>,
    given_id: Option<uuid::Uuid>,
) -> Value {
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
    map.insert("source".to_owned(), build_payment_source(req, source_type));
    if !req.splits.is_empty() {
        map.insert("splits".to_owned(), build_splits_json(req));
    }
    Value::Object(map)
}

fn build_payment_source(req: &CreatePaymentRequest, source_type: Option<&str>) -> Value {
    let mut source = Map::new();
    let kind = source_type.unwrap_or("token");
    source.insert("type".to_owned(), json!(kind));
    if matches!(req.capture_mode, CaptureMode::Manual) {
        source.insert("manual".to_owned(), json!(true));
    }
    if let Some(token) = req.payment_token.as_deref() {
        if kind.eq_ignore_ascii_case("token") {
            source.insert("token".to_owned(), json!(token));
        }
    }
    Value::Object(source)
}

fn build_splits_json(req: &CreatePaymentRequest) -> Value {
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
    Value::Array(items)
}

fn build_invoice_body(req: &CheckoutSessionRequest) -> Value {
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
    Value::Object(map)
}

fn extract_payment_uuid(
    reference: &ProviderReference,
    provider_id: &ProviderId,
) -> Result<uuid::Uuid, PaymentError> {
    let raw_id = reference
        .payment_id()
        .ok_or_else(|| PaymentError::validation("ProviderReference requires payment_id"))?;
    validate_uuid_id(raw_id, "payment_id").map_err(|err| map_local_error(provider_id, err))
}

async fn parse_json_response(
    provider_id: &ProviderId,
    resp: reqwest::Response,
) -> Result<Value, PaymentError> {
    let status = resp.status();
    let retry_after = parse_retry_after(&resp);
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(classify_http_error(
            provider_id,
            status.as_u16(),
            &body,
            retry_after,
        ));
    }
    serde_json::from_str(&body).map_err(|_| PaymentError::ProviderError {
        provider: provider_id.clone(),
        http_status: Some(status.as_u16()),
        vendor_code: None,
        message: "invalid JSON response from Moyasar".to_owned(),
    })
}

fn parse_retry_after(resp: &reqwest::Response) -> Option<u64> {
    resp.headers()
        .get("retry-after")
        .and_then(|val| val.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
}

fn classify_http_error(
    provider_id: &ProviderId,
    status: u16,
    body: &str,
    retry_after: Option<u64>,
) -> PaymentError {
    let parsed = serde_json::from_str::<RawMoyasarError>(body).ok();
    let err_type = parsed.as_ref().and_then(|p| p.error_type.as_deref());
    let err_code = parsed.as_ref().and_then(|p| p.code.as_deref());
    let message = parsed
        .as_ref()
        .and_then(|p| p.message.clone())
        .unwrap_or_else(|| format!("Moyasar HTTP {status}"));
    let classified = map_moyasar_error(Some(status), err_type, err_code);
    map_classified_error(provider_id, classified, message, retry_after)
}

fn map_classified_error(
    provider_id: &ProviderId,
    classified: ClassifiedMoyasarError,
    message: String,
    retry_after: Option<u64>,
) -> PaymentError {
    let provider = provider_id.clone();
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

fn map_transport_error(provider_id: &ProviderId, err: &reqwest::Error) -> PaymentError {
    PaymentError::ProviderUnavailable {
        provider: provider_id.clone(),
        http_status: None,
        indeterminate: true,
        message: if err.is_timeout() {
            "Moyasar request timed out".to_owned()
        } else {
            "Moyasar transport failure".to_owned()
        },
    }
}

fn map_local_error(provider_id: &ProviderId, err: MoyasarError) -> PaymentError {
    match err {
        MoyasarError::InvalidKey | MoyasarError::KeyTypeMismatch => {
            PaymentError::AuthenticationError {
                provider: provider_id.clone(),
                message: err.to_string(),
            }
        }
        MoyasarError::InvalidWebhookSecret => PaymentError::WebhookVerificationFailed {
            provider: provider_id.clone(),
            reason: err.to_string(),
        },
        other => PaymentError::validation(other.to_string()),
    }
}

fn parse_payment_result(
    provider_id: &ProviderId,
    payload: &Value,
    order_id: Option<&str>,
    livemode: bool,
) -> Result<PaymentResult, PaymentError> {
    let raw: RawMoyasarPayment =
        serde_json::from_value(payload.clone()).map_err(|_| PaymentError::ProviderError {
            provider: provider_id.clone(),
            http_status: None,
            vendor_code: None,
            message: "unexpected Moyasar payment response shape".to_owned(),
        })?;
    let snapshot = build_payment_snapshot(&raw);
    let outcome =
        map_moyasar_status(&snapshot).map_err(|err| map_local_error(provider_id, err))?;
    let reference = build_payment_reference(provider_id, &raw.id, order_id)?;
    let money = Money::new(raw.amount, &raw.currency)?;
    Ok(PaymentResult {
        reference,
        status: map_status(outcome.status),
        money,
        captured_amount: raw.captured,
        refunded_amount: raw.refunded,
        next_action: outcome.next_action.map(map_next_action),
        checkout_url: None,
        livemode,
    })
}

fn build_payment_snapshot(raw: &RawMoyasarPayment) -> MoyasarPaymentSnapshot<'_> {
    let source = raw.source.as_ref();
    let bill_reference = source.and_then(|s| {
        s.bill_reference
            .as_deref()
            .or(s.bill_number.as_deref())
            .or(s.reference_number.as_deref())
    });
    MoyasarPaymentSnapshot {
        status: &raw.status,
        source_type: source.and_then(|s| s.source_type.as_deref()),
        transaction_url: source.and_then(|s| s.transaction_url.as_deref()),
        bill_reference,
    }
}

fn build_payment_reference(
    provider_id: &ProviderId,
    payment_id: &str,
    order_id: Option<&str>,
) -> Result<ProviderReference, PaymentError> {
    let mut builder = ProviderReference::builder(provider_id.clone()).payment_id(payment_id);
    if let Some(order) = order_id {
        builder = builder.order_id(order);
    }
    builder.build()
}

fn parse_invoice_result(
    provider_id: &ProviderId,
    payload: &Value,
    order_id: &str,
    livemode: bool,
) -> Result<PaymentResult, PaymentError> {
    let raw: RawMoyasarInvoice =
        serde_json::from_value(payload.clone()).map_err(|_| PaymentError::ProviderError {
            provider: provider_id.clone(),
            http_status: None,
            vendor_code: None,
            message: "unexpected Moyasar invoice response shape".to_owned(),
        })?;
    let outcome = map_invoice_checkout(InvoiceCheckoutInput {
        invoice_id: &raw.id,
        invoice_url: raw.url.as_deref(),
        amount: raw.amount,
        currency: &raw.currency,
    })
    .map_err(|err| map_local_error(provider_id, err))?;
    let reference = ProviderReference::builder(provider_id.clone())
        .checkout_session_id(&outcome.invoice_id)
        .order_id(order_id)
        .build()?;
    let money = Money::new(outcome.amount, &outcome.currency)?;
    Ok(PaymentResult {
        reference,
        status: PaymentStatus::RequiresAction,
        money,
        captured_amount: None,
        refunded_amount: None,
        next_action: Some(NextAction::Redirect {
            url: outcome.checkout_url.clone(),
        }),
        checkout_url: Some(outcome.checkout_url),
        livemode,
    })
}

fn parse_refund_result(
    provider_id: &ProviderId,
    payload: &Value,
    req: &RefundRequest,
) -> Result<RefundResult, PaymentError> {
    let raw: RawMoyasarPayment =
        serde_json::from_value(payload.clone()).map_err(|_| PaymentError::ProviderError {
            provider: provider_id.clone(),
            http_status: None,
            vendor_code: None,
            message: "unexpected Moyasar refund response shape".to_owned(),
        })?;
    let status = if raw.status.eq_ignore_ascii_case("failed") {
        RefundStatus::Failed
    } else {
        RefundStatus::Succeeded
    };
    let money = match req.amount.as_ref() {
        Some(partial) => partial.clone(),
        None => {
            let amt = raw.refunded.filter(|&v| v > 0).unwrap_or(raw.amount);
            Money::new(amt, &raw.currency)?
        }
    };
    Ok(RefundResult {
        refund_id: raw.id,
        payment_reference: req.reference.clone(),
        status,
        money,
        reason_detail: None,
    })
}

fn map_status(mapped: MappedPaymentStatus) -> PaymentStatus {
    MAPPED_STATUS_TABLE
        .iter()
        .find(|(src, _)| *src == mapped)
        .map_or(PaymentStatus::Initiated, |(_, dst)| *dst)
}

fn map_next_action(action: MappedNextAction) -> NextAction {
    match action {
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
    }
}

fn map_webhook_event_status(event: NormalizedMoyasarEvent) -> Option<PaymentStatus> {
    WEBHOOK_STATUS_TABLE
        .iter()
        .find(|(src, _)| *src == event)
        .map(|(_, status)| *status)
}

fn extract_challenge_if_handshake(
    event: Option<NormalizedMoyasarEvent>,
    payload: &Value,
    provider_id: &ProviderId,
) -> Result<Option<VerificationChallenge>, PaymentError> {
    if event != Some(NormalizedMoyasarEvent::WebhookVerification) {
        return Ok(None);
    }
    let challenge = extract_verification_challenge(payload)
        .map_err(|err| map_local_error(provider_id, err))?;
    Ok(Some(VerificationChallenge {
        webhook_id: challenge.webhook_id,
        verification_code: challenge.verification_code,
    }))
}

fn extract_webhook_reference(provider_id: &ProviderId, payload: &Value) -> Option<ProviderReference> {
    let data = payload.get("data")?;
    let payment_id = data.get("id").and_then(Value::as_str)?;
    ProviderReference::builder(provider_id.clone())
        .payment_id(payment_id)
        .build()
        .ok()
}

fn extract_webhook_money(payload: &Value) -> Option<Money> {
    let data = payload.get("data")?;
    let amount = data.get("amount").and_then(Value::as_u64)?;
    let currency = data.get("currency").and_then(Value::as_str)?;
    Money::new(amount, currency).ok()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use some_payment_core::{
        CaptureMode, CaptureRequest, CheckoutSessionRequest, CreatePaymentRequest, HttpMethod,
        Money, PaymentCapabilities, PaymentError, PaymentProvider, PaymentStatus, ProviderId,
        ProviderReference, RawWebhookRequest, RefundRequest, VoidRequest,
    };

    use super::{MoyasarProvider, MoyasarProviderConfig};

    const VALID_UUID: &str = "550e8400-e29b-41d4-a716-446655440000";

    fn sample_reference() -> ProviderReference {
        ProviderReference::builder(ProviderId::new("moyasar"))
            .payment_id(VALID_UUID)
            .build()
            .expect("valid reference")
    }

    #[test]
    fn provider_id_and_capabilities_match_moyasar_contract() {
        let provider = MoyasarProvider::from_api_key("sk_test_cap_check").expect("valid key");
        assert_eq!(provider.id().as_str(), "moyasar");
        let caps = provider.capabilities();
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
    }

    #[test]
    fn livemode_is_derived_strictly_from_key_prefix() {
        let sk_live = MoyasarProvider::from_api_key("sk_live_abc123").expect("valid");
        let sk_test = MoyasarProvider::from_api_key("sk_test_abc123").expect("valid");
        let pk_live = MoyasarProvider::from_api_key("pk_live_abc123").expect("valid");
        let pk_test = MoyasarProvider::from_api_key("pk_test_abc123").expect("valid");
        assert!(sk_live.livemode());
        assert!(!sk_test.livemode());
        assert!(pk_live.livemode());
        assert!(!pk_test.livemode());
        assert!(MoyasarProvider::from_api_key("bad_prefix_123").is_err());
    }

    #[test]
    fn debug_output_redacts_api_key_and_webhook_secret() {
        let cfg = MoyasarProviderConfig::new("sk_live_canary_api_999")
            .expect("valid")
            .with_webhook_secret("wh_canary_secret_888");
        let cfg_dbg = format!("{cfg:?}");
        assert!(cfg_dbg.contains("[REDACTED]"));
        assert!(!cfg_dbg.contains("sk_live_canary_api_999"));
        assert!(!cfg_dbg.contains("wh_canary_secret_888"));
        let provider = MoyasarProvider::new(cfg).expect("valid");
        let prov_dbg = format!("{provider:?}");
        assert!(prov_dbg.contains("[REDACTED]"));
        assert!(!prov_dbg.contains("sk_live_canary_api_999"));
        assert!(!prov_dbg.contains("wh_canary_secret_888"));
    }

    #[tokio::test]
    async fn publishable_key_fails_closed_locally_on_secret_only_ops() {
        let cfg = MoyasarProviderConfig::new("pk_test_local_guard")
            .expect("valid")
            .with_base_url("http://127.0.0.1:1");
        let provider = MoyasarProvider::new(cfg).expect("valid");
        let reference = sample_reference();

        let get_err = provider.get_payment(&reference).await.unwrap_err();
        assert!(matches!(get_err, PaymentError::AuthenticationError { .. }));

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
        let checkout_err = provider.create_checkout(&checkout_req).await.unwrap_err();
        assert!(matches!(
            checkout_err,
            PaymentError::AuthenticationError { .. }
        ));

        let cap_req = CaptureRequest {
            reference: reference.clone(),
            amount: None,
            idempotency_key: None,
        };
        assert!(matches!(
            provider.capture(&cap_req).await.unwrap_err(),
            PaymentError::AuthenticationError { .. }
        ));

        let void_req = VoidRequest {
            reference: reference.clone(),
            reason: None,
            idempotency_key: None,
        };
        assert!(matches!(
            provider.void_payment(&void_req).await.unwrap_err(),
            PaymentError::AuthenticationError { .. }
        ));

        let refund_req = RefundRequest {
            reference,
            amount: None,
            reason: None,
            idempotency_key: None,
        };
        assert!(matches!(
            provider.refund(&refund_req).await.unwrap_err(),
            PaymentError::AuthenticationError { .. }
        ));
    }

    #[tokio::test]
    async fn create_payment_denies_raw_creditcard_by_default() {
        let cfg = MoyasarProviderConfig::new("sk_test_raw_pan_guard")
            .expect("valid")
            .with_base_url("http://127.0.0.1:1");
        let provider = MoyasarProvider::new(cfg).expect("valid");
        let mut req = CreatePaymentRequest::new(Money::new(10_000, "SAR").expect("money"), "ORD-1");
        req.metadata
            .insert("source_type".to_owned(), "creditcard".to_owned());
        let err = provider.create_payment(&req).await.unwrap_err();
        assert!(matches!(err, PaymentError::ValidationError { .. }));
    }

    #[tokio::test]
    async fn parse_and_verify_webhook_scrubs_secret_and_handles_typo_and_handshake() {
        let cfg = MoyasarProviderConfig::new("sk_test_wh_verify")
            .expect("valid")
            .with_webhook_secret("wh_secret_canary_777");
        let provider = MoyasarProvider::new(cfg).expect("valid");

        let faild_body = br#"{
            "id": "evt_typo_1",
            "type": "payment_faild",
            "live": false,
            "secret_token": "wh_secret_canary_777",
            "data": { "id": "550e8400-e29b-41d4-a716-446655440000", "amount": 2500, "currency": "SAR" }
        }"#;
        let raw_faild = RawWebhookRequest {
            raw_body: faild_body.to_vec(),
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

        let handshake_body = br#"{
            "id": "evt_hs_1",
            "type": "webhook_verification",
            "live": true,
            "secret_token": "wh_secret_canary_777",
            "data": { "webhook_id": "wh_live_42", "verification_code": "123456" }
        }"#;
        let raw_hs = RawWebhookRequest {
            raw_body: handshake_body.to_vec(),
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
