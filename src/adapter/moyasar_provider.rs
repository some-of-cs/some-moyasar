//! Optional `some-payment-core::PaymentProvider` implementation for Moyasar (US-9.2.0 Unit 3).

use core::fmt;

use async_trait::async_trait;
use reqwest::Method;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
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

const MOYASAR_SLUG: &str = "moyasar";

/// Configuration for [`MoyasarProvider`] with key-prefix validation and secret redaction.
///
/// ```
/// use some_moyasar::adapter::moyasar_provider::MoyasarProviderConfig;
///
/// let cfg = MoyasarProviderConfig::new("sk_test_example123")
///     .expect("valid")
///     .with_webhook_secret("wh_secret_123");
/// assert!(format!("{cfg:?}").contains("[REDACTED]"));
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
    /// Creates a config validated via [`detect_key_mode`] (`allow_raw_cards = false` by default).
    ///
    /// # Errors
    ///
    /// Returns [`PaymentError::ConfigurationError`] if `api_key` has an invalid prefix.
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

    /// Sets the shared webhook secret for constant-time `secret_token` verification.
    #[must_use]
    pub fn with_webhook_secret(mut self, secret: impl Into<String>) -> Self {
        self.webhook_secret = Some(SecretString::from(secret.into()));
        self
    }

    /// Overrides the Moyasar API base URL (for example, an offline `wiremock` URI).
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

    /// Opts in or out of raw `creditcard` PAN submission (defaults to `false`).
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

/// Optional [`PaymentProvider`] adapter backed by Moyasar REST API v1 and offline alignment guards.
///
/// ```
/// use some_moyasar::adapter::moyasar_provider::MoyasarProvider;
/// use some_payment_core::{PaymentCapabilities, PaymentProvider};
///
/// let provider = MoyasarProvider::from_api_key("sk_test_example123").expect("valid");
/// assert_eq!(provider.id().as_str(), "moyasar");
/// assert!(!provider.livemode());
/// assert!(
///     provider
///         .capabilities()
///         .contains(PaymentCapabilities::NATIVE_CREATE_IDEMPOTENCY)
/// );
/// ```
#[derive(Clone)]
pub struct MoyasarProvider {
    provider_id: ProviderId,
    config: MoyasarProviderConfig,
    livemode: bool,
}

impl MoyasarProvider {
    /// Constructs a [`MoyasarProvider`] from a validated [`MoyasarProviderConfig`].
    ///
    /// # Errors
    ///
    /// Returns [`PaymentError::ConfigurationError`] if the API key fails [`detect_key_mode`].
    pub fn new(config: MoyasarProviderConfig) -> Result<Self, PaymentError> {
        let cls = detect_key_mode(config.api_key.expose_secret())
            .map_err(|_| PaymentError::configuration("invalid Moyasar API key prefix"))?;
        Ok(Self {
            provider_id: ProviderId::new(MOYASAR_SLUG),
            config,
            livemode: cls.mode.is_live(),
        })
    }

    /// Convenience constructor building a [`MoyasarProvider`] with defaults from a raw API key.
    ///
    /// # Errors
    ///
    /// Returns [`PaymentError::ConfigurationError`] if `api_key` fails [`detect_key_mode`].
    pub fn from_api_key(api_key: impl Into<String>) -> Result<Self, PaymentError> {
        MoyasarProviderConfig::new(api_key).and_then(Self::new)
    }

    /// Returns `true` when operating in Moyasar's live environment (`sk_live_` / `pk_live_`).
    #[must_use]
    pub fn livemode(&self) -> bool {
        self.livemode
    }

    /// Returns the configured Moyasar API base URL.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }

    fn map_err(&self, err: MoyasarError) -> PaymentError {
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

    fn decode<T: DeserializeOwned>(&self, payload: Value, kind: &str) -> Result<T, PaymentError> {
        serde_json::from_value(payload).map_err(|_| PaymentError::ProviderError {
            provider: self.provider_id.clone(),
            http_status: None,
            vendor_code: None,
            message: format!("unexpected Moyasar {kind} response shape"),
        })
    }

    async fn send_json(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, PaymentError> {
        let mut req = self
            .config
            .http_client
            .request(method, format!("{}{path}", self.config.base_url))
            .basic_auth(self.config.api_key.expose_secret(), Some(""));
        if let Some(payload) = body {
            req = req.json(payload);
        }
        let resp = req
            .send()
            .await
            .map_err(|_| PaymentError::ProviderUnavailable {
                provider: self.provider_id.clone(),
                http_status: None,
                indeterminate: true,
                message: "Moyasar transport failure".to_owned(),
            })?;
        let status = resp.status();
        let retry_after = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
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

    async fn post_mutation(
        &self,
        op: SecretOnlyOperation,
        reference: &ProviderReference,
        suffix: &str,
        amount: Option<&Money>,
    ) -> Result<Value, PaymentError> {
        require_secret_key(self.config.api_key.expose_secret(), op)
            .map_err(|e| self.map_err(e))?;
        let raw_id = reference
            .payment_id()
            .ok_or_else(|| PaymentError::validation("ProviderReference requires payment_id"))?;
        let uuid = validate_uuid_id(raw_id, "payment_id").map_err(|e| self.map_err(e))?;
        let partial = validate_partial_amount(amount.map(Money::amount))
            .map_err(|e| self.map_err(e))?;
        let body = build_mutation_body(partial);
        let path = format!("/payments/{uuid}/{suffix}");
        self.send_json(Method::POST, &path, body.as_ref()).await
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
        let msg = parsed
            .as_ref()
            .and_then(|v| v.get("message")?.as_str())
            .map_or_else(|| format!("Moyasar HTTP {status}"), str::to_owned);
        let c = map_moyasar_error(Some(status), err_type, err_code);
        self.map_classified(c, msg, retry_after)
    }

    fn map_classified(
        &self,
        c: ClassifiedMoyasarError,
        message: String,
        retry_after: Option<u64>,
    ) -> PaymentError {
        let p = self.provider_id.clone();
        match c.class {
            MoyasarErrorClass::ThreeDsFailed => PaymentError::ThreeDsFailed {
                provider: p,
                code: c.vendor_code,
                message,
            },
            MoyasarErrorClass::AuthenticationError => {
                PaymentError::AuthenticationError { provider: p, message }
            }
            MoyasarErrorClass::ValidationError => PaymentError::ValidationError { message },
            MoyasarErrorClass::CardDeclined => PaymentError::CardDeclined {
                provider: p,
                decline_code: c.vendor_code,
                advice_code: None,
                message,
            },
            MoyasarErrorClass::RateLimited => PaymentError::RateLimited {
                provider: p,
                retry_after_secs: retry_after,
            },
            MoyasarErrorClass::ProviderUnavailable => PaymentError::ProviderUnavailable {
                provider: p,
                http_status: c.http_status,
                indeterminate: c.indeterminate,
                message,
            },
            MoyasarErrorClass::ProviderError => PaymentError::ProviderError {
                provider: p,
                http_status: c.http_status,
                vendor_code: c.vendor_code,
                message,
            },
        }
    }

    fn parse_payment(
        &self,
        payload: Value,
        order_id: Option<&str>,
    ) -> Result<PaymentResult, PaymentError> {
        let raw: RawPayment = self.decode(payload, "payment")?;
        let src = raw.source.as_ref();
        let snap = MoyasarPaymentSnapshot {
            status: &raw.status,
            source_type: src.and_then(|s| s.source_type.as_deref()),
            transaction_url: src.and_then(|s| s.transaction_url.as_deref()),
            bill_reference: src.and_then(|s| {
                s.bill_reference
                    .as_deref()
                    .or(s.bill_number.as_deref())
                    .or(s.reference_number.as_deref())
            }),
        };
        let outcome = map_moyasar_status(&snap).map_err(|e| self.map_err(e))?;
        let mut rb = ProviderReference::builder(self.provider_id.clone()).payment_id(&raw.id);
        if let Some(order) = order_id {
            rb = rb.order_id(order);
        }
        Ok(PaymentResult {
            reference: rb.build()?,
            status: to_payment_status(outcome.status),
            money: Money::new(raw.amount, &raw.currency)?,
            captured_amount: raw.captured,
            refunded_amount: raw.refunded,
            next_action: outcome.next_action.map(to_next_action),
            checkout_url: None,
            livemode: self.livemode,
        })
    }

    fn parse_refund(
        &self,
        payload: Value,
        req: &RefundRequest,
    ) -> Result<RefundResult, PaymentError> {
        let raw: RawPayment = self.decode(payload, "refund")?;
        let status = if raw.status.eq_ignore_ascii_case("failed") {
            RefundStatus::Failed
        } else {
            RefundStatus::Succeeded
        };
        let money = match req.amount.as_ref() {
            Some(p) => p.clone(),
            None => Money::new(
                raw.refunded.filter(|&v| v > 0).unwrap_or(raw.amount),
                &raw.currency,
            )?,
        };
        Ok(RefundResult {
            refund_id: raw.id,
            payment_reference: req.reference.clone(),
            status,
            money,
            reason_detail: None,
        })
    }
}

impl fmt::Debug for MoyasarProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MoyasarProvider")
            .field("provider_id", &self.provider_id)
            .field("config", &self.config)
            .field("livemode", &self.livemode)
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
        let source_type = resolve_source(req);
        if let Some(st) = source_type {
            let policy = RawCardPolicy {
                allow_raw_cards: self.config.allow_raw_cards,
            };
            guard_raw_pan(st, policy).map_err(|e| self.map_err(e))?;
        }
        let splits: Vec<SplitCheckItem> = req
            .splits
            .iter()
            .map(|s| SplitCheckItem {
                recipient_id: s.recipient_id.clone(),
                amount: s.amount,
            })
            .collect();
        validate_moyasar_preflight(&MoyasarPreflightInput {
            payment_id: None,
            amount: req.money.amount(),
            manual_capture: matches!(req.capture_mode, CaptureMode::Manual),
            source_type,
            metadata: &req.metadata,
            splits: &splits,
        })
        .map_err(|e| self.map_err(e))?;
        let given_id = req
            .idempotency_key
            .as_deref()
            .map(|raw| validate_given_id(raw).map_err(|e| self.map_err(e)))
            .transpose()?;
        let body = build_payment_json(req, source_type, given_id);
        let payload = self.send_json(Method::POST, "/payments", Some(&body)).await?;
        self.parse_payment(payload, Some(&req.order_id))
    }

    async fn get_payment(
        &self,
        reference: &ProviderReference,
    ) -> Result<PaymentResult, PaymentError> {
        require_secret_key(
            self.config.api_key.expose_secret(),
            SecretOnlyOperation::RetrievePayment,
        )
        .map_err(|e| self.map_err(e))?;
        let raw_id = reference
            .payment_id()
            .ok_or_else(|| PaymentError::validation("ProviderReference requires payment_id"))?;
        let uuid = validate_uuid_id(raw_id, "payment_id").map_err(|e| self.map_err(e))?;
        let payload = self
            .send_json(Method::GET, &format!("/payments/{uuid}"), None)
            .await?;
        self.parse_payment(payload, reference.order_id())
    }

    async fn create_checkout(
        &self,
        req: &CheckoutSessionRequest,
    ) -> Result<PaymentResult, PaymentError> {
        require_secret_key(
            self.config.api_key.expose_secret(),
            SecretOnlyOperation::CreateInvoice,
        )
        .map_err(|e| self.map_err(e))?;
        req.validate()?;
        validate_metadata_limits(&req.metadata).map_err(|e| self.map_err(e))?;
        let mut body = json!({
            "amount": req.money.amount(),
            "currency": req.money.currency().as_str(),
            "description": req.description.as_deref().unwrap_or(&req.order_id),
            "success_url": &req.success_url,
        });
        if let Some(cancel) = req.cancel_url.as_deref() {
            body["back_url"] = json!(cancel);
        }
        if !req.metadata.is_empty() {
            body["metadata"] = json!(&req.metadata);
        }
        let payload = self.send_json(Method::POST, "/invoices", Some(&body)).await?;
        let raw: RawInvoice = self.decode(payload, "invoice")?;
        let out = map_invoice_checkout(InvoiceCheckoutInput {
            invoice_id: &raw.id,
            invoice_url: raw.url.as_deref(),
            amount: raw.amount,
            currency: &raw.currency,
        })
        .map_err(|e| self.map_err(e))?;
        let reference = ProviderReference::builder(self.provider_id.clone())
            .checkout_session_id(&out.invoice_id)
            .order_id(&req.order_id)
            .build()?;
        Ok(PaymentResult {
            reference,
            status: PaymentStatus::RequiresAction,
            money: Money::new(out.amount, &out.currency)?,
            captured_amount: None,
            refunded_amount: None,
            next_action: Some(NextAction::Redirect {
                url: out.checkout_url.clone(),
            }),
            checkout_url: Some(out.checkout_url),
            livemode: self.livemode,
        })
    }

    async fn capture(&self, req: &CaptureRequest) -> Result<PaymentResult, PaymentError> {
        req.validate()?;
        let p = self
            .post_mutation(
                SecretOnlyOperation::Capture,
                &req.reference,
                "capture",
                req.amount.as_ref(),
            )
            .await?;
        self.parse_payment(p, req.reference.order_id())
    }

    async fn void_payment(&self, req: &VoidRequest) -> Result<PaymentResult, PaymentError> {
        req.validate()?;
        let p = self
            .post_mutation(SecretOnlyOperation::Void, &req.reference, "void", None)
            .await?;
        self.parse_payment(p, req.reference.order_id())
    }

    async fn refund(&self, req: &RefundRequest) -> Result<RefundResult, PaymentError> {
        req.validate()?;
        let p = self
            .post_mutation(
                SecretOnlyOperation::Refund,
                &req.reference,
                "refund",
                req.amount.as_ref(),
            )
            .await?;
        self.parse_refund(p, req)
    }

    async fn parse_and_verify_webhook(
        &self,
        raw: &RawWebhookRequest,
    ) -> Result<NormalizedWebhookEvent, PaymentError> {
        let secret = self
            .config
            .webhook_secret
            .as_ref()
            .ok_or_else(|| PaymentError::configuration("webhook_secret is not configured"))?;
        let v = verify_and_scrub_moyasar_webhook(&raw.raw_body, secret.expose_secret())
            .map_err(|e| self.map_err(e))?;
        let norm = normalize_moyasar_event(&v.event_type);
        let verification_challenge = if norm == Some(NormalizedMoyasarEvent::WebhookVerification) {
            let ch = extract_verification_challenge(&v.raw_redacted)
                .map_err(|e| self.map_err(e))?;
            Some(VerificationChallenge {
                webhook_id: ch.webhook_id.clone(),
                verification_code: ch.verification_code.clone(),
            })
        } else {
            None
        };
        let data = v.raw_redacted.get("data");
        let reference = data.and_then(|d| d.get("id")?.as_str()).and_then(|pid| {
            ProviderReference::builder(self.provider_id.clone())
                .payment_id(pid)
                .build()
                .ok()
        });
        let money = data
            .and_then(|d| d.get("amount")?.as_u64().zip(d.get("currency")?.as_str()))
            .and_then(|(amt, cur)| Money::new(amt, cur).ok());
        Ok(NormalizedWebhookEvent {
            provider_id: self.provider_id.clone(),
            event_id: v.event_id,
            event_type: v.event_type,
            livemode: v.livemode,
            requires_hydration: false,
            reference,
            status: norm.and_then(to_webhook_status),
            money,
            verification_challenge,
            raw_redacted: v.raw_redacted,
        })
    }
}

#[derive(Deserialize)]
struct RawSource {
    #[serde(rename = "type")]
    source_type: Option<String>,
    transaction_url: Option<String>,
    bill_reference: Option<String>,
    bill_number: Option<String>,
    reference_number: Option<String>,
}

#[derive(Deserialize)]
struct RawPayment {
    id: String,
    status: String,
    amount: u64,
    currency: String,
    captured: Option<u64>,
    refunded: Option<u64>,
    source: Option<RawSource>,
}

#[derive(Deserialize)]
struct RawInvoice {
    id: String,
    url: Option<String>,
    amount: u64,
    currency: String,
}

fn resolve_source(req: &CreatePaymentRequest) -> Option<&str> {
    if let Some(s) = req.metadata.get("source_type").map(String::as_str) {
        return Some(s);
    }
    let t = req.payment_token.as_deref().map(str::trim)?;
    if t.eq_ignore_ascii_case("creditcard") || t.eq_ignore_ascii_case("credit_card") {
        return Some("creditcard");
    }
    Some("token")
}

fn build_payment_json(
    req: &CreatePaymentRequest,
    source_type: Option<&str>,
    given_id: Option<uuid::Uuid>,
) -> Value {
    let kind = source_type.unwrap_or("token");
    let mut src = json!({ "type": kind });
    if matches!(req.capture_mode, CaptureMode::Manual) {
        src["manual"] = json!(true);
    }
    if let Some(tok) = req
        .payment_token
        .as_deref()
        .filter(|_| kind.eq_ignore_ascii_case("token"))
    {
        src["token"] = json!(tok);
    }
    let mut body = json!({
        "amount": req.money.amount(),
        "currency": req.money.currency().as_str(),
        "description": req.description.as_deref().unwrap_or(&req.order_id),
        "source": src,
    });
    if let Some(url) = req.callback_url.as_deref() {
        body["callback_url"] = json!(url);
    }
    if let Some(id) = given_id {
        body["given_id"] = json!(id.to_string());
    }
    if !req.metadata.is_empty() {
        body["metadata"] = json!(&req.metadata);
    }
    if !req.splits.is_empty() {
        body["splits"] = Value::Array(
            req.splits
                .iter()
                .map(|s| {
                    json!({
                        "recipient_id": s.recipient_id,
                        "amount": s.amount,
                        "fee_source": s.fee_source,
                    })
                })
                .collect(),
        );
    }
    body
}

fn to_payment_status(mapped: MappedPaymentStatus) -> PaymentStatus {
    match mapped {
        MappedPaymentStatus::Initiated => PaymentStatus::Initiated,
        MappedPaymentStatus::RequiresAction => PaymentStatus::RequiresAction,
        MappedPaymentStatus::Authorized => PaymentStatus::Authorized,
        MappedPaymentStatus::Captured => PaymentStatus::Captured,
        MappedPaymentStatus::Paid => PaymentStatus::Paid,
        MappedPaymentStatus::Refunded => PaymentStatus::Refunded,
        MappedPaymentStatus::Voided => PaymentStatus::Voided,
        MappedPaymentStatus::Failed => PaymentStatus::Failed,
        MappedPaymentStatus::SetupCompleted => PaymentStatus::SetupCompleted,
    }
}

fn to_next_action(action: MappedNextAction) -> NextAction {
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

fn to_webhook_status(event: NormalizedMoyasarEvent) -> Option<PaymentStatus> {
    match event {
        NormalizedMoyasarEvent::PaymentPaid => Some(PaymentStatus::Paid),
        NormalizedMoyasarEvent::PaymentFailed => Some(PaymentStatus::Failed),
        NormalizedMoyasarEvent::PaymentAuthorized => Some(PaymentStatus::Authorized),
        NormalizedMoyasarEvent::PaymentCaptured => Some(PaymentStatus::Captured),
        NormalizedMoyasarEvent::PaymentRefunded => Some(PaymentStatus::Refunded),
        NormalizedMoyasarEvent::PaymentVoided => Some(PaymentStatus::Voided),
        NormalizedMoyasarEvent::PaymentAbandoned => Some(PaymentStatus::Canceled),
        NormalizedMoyasarEvent::PaymentVerified => Some(PaymentStatus::SetupCompleted),
        _ => None,
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
        assert!(format!("{cfg:?}").contains("[REDACTED]"));
        assert!(!format!("{cfg:?}").contains("sk_live_canary_api_999"));
        assert!(!format!("{cfg:?}").contains("wh_canary_secret_888"));

        let live = MoyasarProvider::new(cfg).expect("valid");
        assert!(format!("{live:?}").contains("[REDACTED]"));
        assert!(!format!("{live:?}").contains("sk_live_canary_api_999"));
        assert_eq!(live.id().as_str(), "moyasar");
        assert!(live.livemode());

        let caps = live.capabilities();
        assert!(caps.contains(PaymentCapabilities::NATIVE_CREATE_IDEMPOTENCY));
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
    async fn local_fail_closed_guards_and_webhook_scrubbing() {
        let pk_cfg = MoyasarProviderConfig::new("pk_test_local_guard")
            .expect("valid")
            .with_base_url("http://127.0.0.1:1");
        let pk = MoyasarProvider::new(pk_cfg).expect("valid");
        let reference = ProviderReference::builder(ProviderId::new("moyasar"))
            .payment_id(VALID_UUID)
            .build()
            .expect("valid reference");

        assert!(matches!(
            pk.get_payment(&reference).await.unwrap_err(),
            PaymentError::AuthenticationError { .. }
        ));
        let checkout = CheckoutSessionRequest {
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
            pk.create_checkout(&checkout).await.unwrap_err(),
            PaymentError::AuthenticationError { .. }
        ));
        let cap = CaptureRequest {
            reference: reference.clone(),
            amount: None,
            idempotency_key: None,
        };
        assert!(matches!(
            pk.capture(&cap).await.unwrap_err(),
            PaymentError::AuthenticationError { .. }
        ));
        let void_req = VoidRequest {
            reference: reference.clone(),
            reason: None,
            idempotency_key: None,
        };
        assert!(matches!(
            pk.void_payment(&void_req).await.unwrap_err(),
            PaymentError::AuthenticationError { .. }
        ));
        let refund_req = RefundRequest {
            reference,
            amount: None,
            reason: None,
            idempotency_key: None,
        };
        assert!(matches!(
            pk.refund(&refund_req).await.unwrap_err(),
            PaymentError::AuthenticationError { .. }
        ));

        let sk_cfg = MoyasarProviderConfig::new("sk_test_wh_verify")
            .expect("valid")
            .with_base_url("http://127.0.0.1:1")
            .with_webhook_secret("wh_secret_canary_777");
        let sk = MoyasarProvider::new(sk_cfg).expect("valid");
        let mut req = CreatePaymentRequest::new(Money::new(10_000, "SAR").expect("money"), "ORD-1");
        req.metadata
            .insert("source_type".to_owned(), "creditcard".to_owned());
        assert!(matches!(
            sk.create_payment(&req).await.unwrap_err(),
            PaymentError::ValidationError { .. }
        ));

        let faild_raw = RawWebhookRequest {
            raw_body: json!({
                "id": "evt_typo_1",
                "type": "payment_faild",
                "live": false,
                "secret_token": "wh_secret_canary_777",
                "data": { "id": VALID_UUID, "amount": 2500, "currency": "SAR" }
            })
            .to_string()
            .into_bytes(),
            headers: BTreeMap::new(),
            query_params: BTreeMap::new(),
            method: HttpMethod::Post,
        };
        let event = sk
            .parse_and_verify_webhook(&faild_raw)
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

        let hs_raw = RawWebhookRequest {
            raw_body: json!({
                "id": "evt_hs_1",
                "type": "webhook_verification",
                "live": true,
                "secret_token": "wh_secret_canary_777",
                "data": { "webhook_id": "wh_live_42", "verification_code": "123456" }
            })
            .to_string()
            .into_bytes(),
            headers: BTreeMap::new(),
            query_params: BTreeMap::new(),
            method: HttpMethod::Post,
        };
        let hs = sk.parse_and_verify_webhook(&hs_raw).await.expect("hs");
        let ch = hs.verification_challenge.expect("challenge");
        assert_eq!(ch.webhook_id, "wh_live_42");
        assert_eq!(ch.verification_code, "123456");
        assert!(!hs.raw_redacted.to_string().contains("wh_secret_canary_777"));
    }
}
