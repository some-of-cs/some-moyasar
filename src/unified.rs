//! Unified cross-gateway payment trait adapter for `MoyasarClient`.

/// Orthogonal [`PayoutCapability`] adapter backed by `client.payouts()`.
pub mod payout_adapter;
/// Ergonomic [`MoyasarGatewayAdapter`] router wrapper for [`MoyasarClient`].
pub mod router;
/// Orthogonal [`SplitSettlementCapability`] adapter backed by `client.payments().create(...)`.
pub mod split_adapter;

pub use payout_adapter::{
    PayoutCapability, UnifiedBulkPayoutResponse, UnifiedPayoutDestination, UnifiedPayoutRequest,
    UnifiedPayoutResponse,
};
pub use router::MoyasarGatewayAdapter;
pub use split_adapter::{
    SplitSettlementCapability, UnifiedSplitPaymentRequest, UnifiedSplitPaymentResponse,
    UnifiedSplitRecipientRole, UnifiedSplitRule,
};

use std::fmt::Write as _;
use std::future::Future;
use std::pin::Pin;

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use uuid::Uuid;
use zeroize::Zeroize;

use crate::kernel::auth::SecretKey;
use crate::kernel::client::MoyasarClient;
use crate::kernel::error::MoyasarError;
use crate::kernel::metadata::Metadata;
use crate::kernel::types::Currency;
use crate::resources::payment_intents::CreatePaymentIntentParams;
use crate::resources::payments::{
    CreatePaymentParams, Payment, PaymentId, PaymentSourceRequest, TokenSource, can_void_payment,
    is_void_fallback_eligible, remaining_refundable_amount, validate_refund_amount,
};
use crate::resources::tokens::{TokenId, require_active_token};
use crate::webhook_engine::{ConstantTimeSecretVerifier, REPLAY_TOLERANCE, WebhookEvent};

/// Inbound HTTP header carrying the hex-encoded HMAC-SHA256 payload signature.
pub const WEBHOOK_SIGNATURE_HEADER: &str = "x-moyasar-signature";

/// Legacy inbound HTTP header carrying a static shared webhook secret token.
pub const WEBHOOK_SECRET_TOKEN_HEADER: &str = "x-moyasar-secret-token";

/// Cross-gateway request parameters for creating a hosted/client-secret payment intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnifiedIntentRequest<'a> {
    /// Intent amount in minor units (`u64`).
    pub amount_minor: u64,
    /// ISO 4217 currency code string (e.g., `"SAR"`).
    pub currency: &'a str,
    /// Optional human-readable description.
    pub description: Option<&'a str>,
    /// Redirect callback URL (`https://...`).
    pub callback_url: &'a str,
    /// Optional merchant order reference forwarded in intent metadata.
    pub order_reference: Option<&'a str>,
    /// Optional ISO 3166-1 alpha-2 buyer country code forwarded in intent metadata.
    pub buyer_country: Option<&'a str>,
    /// Optional customer email address forwarded in intent metadata.
    pub customer_email: Option<&'a str>,
    /// Optional customer phone number forwarded in intent metadata.
    pub customer_phone: Option<&'a str>,
    /// Optional caller-supplied UUID idempotency key (`given_id`).
    pub idempotency_key: Option<Uuid>,
}

impl<'a> UnifiedIntentRequest<'a> {
    /// Creates a new [`UnifiedIntentRequest`].
    #[must_use]
    pub const fn new(amount_minor: u64, currency: &'a str, callback_url: &'a str) -> Self {
        Self {
            amount_minor,
            currency,
            description: None,
            callback_url,
            order_reference: None,
            buyer_country: None,
            customer_email: None,
            customer_phone: None,
            idempotency_key: None,
        }
    }

    /// Attaches an optional human-readable description.
    #[must_use]
    pub const fn description(mut self, description: &'a str) -> Self {
        self.description = Some(description);
        self
    }

    /// Attaches an optional merchant order reference forwarded via metadata.
    #[must_use]
    pub const fn order_reference(mut self, order_reference: &'a str) -> Self {
        self.order_reference = Some(order_reference);
        self
    }

    /// Attaches an optional buyer ISO 3166-1 alpha-2 country code forwarded via metadata.
    #[must_use]
    pub const fn buyer_country(mut self, buyer_country: &'a str) -> Self {
        self.buyer_country = Some(buyer_country);
        self
    }

    /// Attaches an optional customer email forwarded via metadata.
    #[must_use]
    pub const fn customer_email(mut self, customer_email: &'a str) -> Self {
        self.customer_email = Some(customer_email);
        self
    }

    /// Attaches an optional customer phone number forwarded via metadata.
    #[must_use]
    pub const fn customer_phone(mut self, customer_phone: &'a str) -> Self {
        self.customer_phone = Some(customer_phone);
        self
    }

    /// Attaches an explicit UUID idempotency key (`given_id`).
    #[must_use]
    pub const fn idempotency_key(mut self, idempotency_key: Uuid) -> Self {
        self.idempotency_key = Some(idempotency_key);
        self
    }
}

/// Cross-gateway request parameters for charging a saved card token off-session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnifiedTokenChargeRequest<'a> {
    /// Charge amount in minor units (`u64`).
    pub amount_minor: u64,
    /// ISO 4217 currency code string (e.g., `"SAR"`).
    pub currency: &'a str,
    /// Saved card token identifier (`"token_..."`).
    pub token_id: &'a str,
    /// Deterministic or caller-supplied UUID idempotency key (`given_id`).
    pub idempotency_key: Uuid,
}

impl<'a> UnifiedTokenChargeRequest<'a> {
    /// Creates a new [`UnifiedTokenChargeRequest`].
    #[must_use]
    pub const fn new(
        amount_minor: u64,
        currency: &'a str,
        token_id: &'a str,
        idempotency_key: Uuid,
    ) -> Self {
        Self {
            amount_minor,
            currency,
            token_id,
            idempotency_key,
        }
    }
}

/// Unified payment intent response across payment gateways.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedIntentResponse {
    /// Gateway intent identifier.
    pub id: String,
    /// Client secret for frontend/mobile SDK fulfillment.
    pub client_secret: Option<String>,
    /// Intent amount in minor units (`u64`).
    pub amount_minor: u64,
    /// ISO 4217 currency code.
    pub currency: Currency,
    /// Intent expiration timestamp (UTC).
    pub expires_at: DateTime<Utc>,
}

/// Unified off-session token charge response across payment gateways.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedChargeResponse {
    /// Gateway transaction/payment identifier.
    pub transaction_id: String,
    /// Normalized status string.
    pub status: String,
    /// Charged amount in minor units (`u64`).
    pub amount_minor: u64,
    /// ISO 4217 currency code.
    pub currency: Currency,
}

/// Unified refund or void reversal response across payment gateways.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedReversalResponse {
    /// Gateway transaction/payment identifier.
    pub transaction_id: String,
    /// `"voided"` or `"refunded"`.
    pub action: String,
    /// Reversed amount in minor units (`u64`) for this operation.
    pub amount_reversed_minor: u64,
    /// ISO 4217 currency code.
    pub currency: Currency,
}

/// Unified verified inbound webhook event across payment gateways.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedWebhookEvent {
    /// Unique event identifier.
    pub id: Uuid,
    /// Canonical event type string.
    pub event_type: String,
    /// Event creation timestamp (UTC).
    pub created_at: DateTime<Utc>,
    /// Live vs test mode flag.
    pub live: bool,
    /// Raw event payload object.
    pub data: serde_json::Value,
}

/// Cross-gateway payment adapter trait (`some-payments` ecosystem).
///
/// # Examples
///
/// ```
/// use some_moyasar::prelude::{MoyasarClient, SecretKey};
/// use some_moyasar::unified::UnifiedPaymentGateway;
///
/// let key = SecretKey::new("sk_test_unified_demo").unwrap();
/// let client = MoyasarClient::new(key).unwrap();
/// fn assert_gateway<G: UnifiedPaymentGateway>(_g: &G) {}
/// assert_gateway(&client);
/// ```
pub trait UnifiedPaymentGateway: Send + Sync {
    /// Concrete gateway error type.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Creates a hosted/client-secret payment intent in minor units.
    fn create_intent<'a>(
        &'a self,
        request: UnifiedIntentRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedIntentResponse, Self::Error>> + Send + 'a>>;

    /// Charges an active saved token off-session with explicit idempotency key normalization.
    fn charge_saved_token<'a>(
        &'a self,
        request: UnifiedTokenChargeRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedChargeResponse, Self::Error>> + Send + 'a>>;

    /// Reverses a transaction via smart void (when full & unsettled) or refund.
    fn refund_or_void<'a>(
        &'a self,
        transaction_id: &'a str,
        amount_minor: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedReversalResponse, Self::Error>> + Send + 'a>>;

    /// Verifies and parses an inbound webhook payload requiring `x-moyasar-signature` HMAC-SHA256.
    fn verify_webhook(
        &self,
        headers: &http::HeaderMap,
        raw_body: &[u8],
    ) -> Result<UnifiedWebhookEvent, Self::Error>;

    /// Verifies and parses a native Moyasar webhook JSON payload using constant-time Double-HMAC
    /// verification over the body's `secret_token` and replay-window validation.
    fn verify_webhook_payload(&self, raw_body: &[u8]) -> Result<UnifiedWebhookEvent, Self::Error>;
}

/// Companion `some-payment` capability marker trait for payment operations.
pub trait PaymentCapability: UnifiedPaymentGateway {}
impl<T: UnifiedPaymentGateway + ?Sized> PaymentCapability for T {}

/// Companion `some-payment` capability trait for webhook verification and normalization.
pub trait WebhookCapability: UnifiedPaymentGateway {
    /// Verifies and normalizes an inbound webhook, using `x-moyasar-signature` HMAC-SHA256 when
    /// that header is present or falling back to native Moyasar `secret_token` payload
    /// verification when no signature header is supplied.
    fn verify_and_normalize(
        &self,
        headers: &http::HeaderMap,
        raw_body: &[u8],
    ) -> Result<UnifiedWebhookEvent, Self::Error> {
        if headers.contains_key(WEBHOOK_SIGNATURE_HEADER) {
            return self.verify_webhook(headers, raw_body);
        }
        self.verify_webhook_payload(raw_body)
    }
}
impl<T: UnifiedPaymentGateway + ?Sized> WebhookCapability for T {}

/// Companion `some-payment` trait alias for constant-time webhook verification.
pub trait WebhookVerifier: UnifiedPaymentGateway {}
impl<T: UnifiedPaymentGateway + ?Sized> WebhookVerifier for T {}

/// Companion `some-payment` trait alias for payout engines.
pub trait PayoutEngine: PayoutCapability {}
impl<T: PayoutCapability + ?Sized> PayoutEngine for T {}

impl UnifiedPaymentGateway for MoyasarClient<SecretKey> {
    type Error = MoyasarError;

    fn create_intent<'a>(
        &'a self,
        request: UnifiedIntentRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedIntentResponse, Self::Error>> + Send + 'a>> {
        Box::pin(async move {
            let params = build_intent_params(request)?;
            let intent = self.payment_intents().create(params).await?;
            Ok(UnifiedIntentResponse {
                id: intent.id.as_str().to_owned(),
                client_secret: intent.client_secret().map(str::to_owned),
                amount_minor: intent.amount,
                currency: intent.currency,
                expires_at: intent.expires_at,
            })
        })
    }

    fn charge_saved_token<'a>(
        &'a self,
        request: UnifiedTokenChargeRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedChargeResponse, Self::Error>> + Send + 'a>> {
        Box::pin(async move {
            let cur: Currency = request.currency.parse()?;
            let token_id = TokenId::new(request.token_id)?;
            let token = self.tokens().retrieve(&token_id).await?;
            require_active_token(token.status)?;

            let mut source = TokenSource::new(token_id.as_str())?;
            source.three_ds = Some(false);
            let params = CreatePaymentParams::builder()
                .amount(request.amount_minor)
                .currency(cur)
                .given_id(request.idempotency_key)
                .payment_source(PaymentSourceRequest::Token(source))
                .build()?;
            let payment = self.payments().create(&params).await?;
            Ok(UnifiedChargeResponse {
                transaction_id: payment.id,
                status: payment.status.as_str().to_owned(),
                amount_minor: payment.amount,
                currency: payment.currency,
            })
        })
    }

    fn refund_or_void<'a>(
        &'a self,
        transaction_id: &'a str,
        amount_minor: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedReversalResponse, Self::Error>> + Send + 'a>>
    {
        Box::pin(async move { execute_unified_reversal(self, transaction_id, amount_minor).await })
    }

    fn verify_webhook(
        &self,
        headers: &http::HeaderMap,
        raw_body: &[u8],
    ) -> Result<UnifiedWebhookEvent, Self::Error> {
        let sig = header_str(headers, WEBHOOK_SIGNATURE_HEADER)
            .ok_or(MoyasarError::InvalidWebhookSignature)?;
        let secret = resolved_webhook_secret(self)?;
        verify_webhook_signature(raw_body, sig, secret)?;
        verify_body_with_secret(secret, raw_body)
    }

    fn verify_webhook_payload(&self, raw_body: &[u8]) -> Result<UnifiedWebhookEvent, Self::Error> {
        let secret = resolved_webhook_secret(self)?;
        verify_body_with_secret(secret, raw_body)
    }
}

impl MoyasarClient<SecretKey> {
    /// Verifies an inbound webhook using the static `x-moyasar-secret-token` header and payload
    /// `secret_token` in constant time.
    ///
    /// # Security Note
    ///
    /// Static secret-token comparison authenticates the sender token and enforces the timestamp
    /// replay window, but unlike [`UnifiedPaymentGateway::verify_webhook`] (which requires an
    /// `x-moyasar-signature` HMAC-SHA256 digest over `raw_body`), a static token header does not
    /// cryptographically bind the raw payload bytes. Prefer [`UnifiedPaymentGateway::verify_webhook`]
    /// whenever HMAC signatures are available.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::InvalidWebhookSecret`] if no webhook secret is configured on the
    /// client, if the `x-moyasar-secret-token` header is missing or mismatches the configured
    /// secret, or if payload verification fails.
    pub fn verify_webhook_with_secret_token(
        &self,
        headers: &http::HeaderMap,
        raw_body: &[u8],
    ) -> Result<UnifiedWebhookEvent, MoyasarError> {
        let token = header_str(headers, WEBHOOK_SECRET_TOKEN_HEADER)
            .ok_or(MoyasarError::InvalidWebhookSecret)?;
        let secret = resolved_webhook_secret(self)?;
        verify_static_secret_token(token, secret)?;
        verify_body_with_secret(secret, raw_body)
    }
}

fn build_intent_params(
    request: UnifiedIntentRequest<'_>,
) -> Result<CreatePaymentIntentParams, MoyasarError> {
    let cur: Currency = request.currency.parse()?;
    let mut params =
        CreatePaymentIntentParams::new(request.amount_minor, cur, request.callback_url);
    if let Some(desc) = request.description {
        params = params.description(desc);
    }
    if let Some(key) = request.idempotency_key {
        params = params.given_id(key);
    }
    if let Some(meta) = build_intent_metadata(&request)? {
        params = params.metadata(meta);
    }
    Ok(params)
}

fn build_intent_metadata(
    request: &UnifiedIntentRequest<'_>,
) -> Result<Option<Metadata>, MoyasarError> {
    let entries = [
        ("order_reference", request.order_reference),
        ("buyer_country", request.buyer_country),
        ("customer_email", request.customer_email),
        ("customer_phone", request.customer_phone),
    ];
    let mut meta = Metadata::new();
    for (key, value) in entries {
        if let Some(val) = value {
            meta.insert(key, val)?;
        }
    }
    Ok((!meta.is_empty()).then_some(meta))
}

fn verify_body_with_secret(
    secret: &str,
    raw_body: &[u8],
) -> Result<UnifiedWebhookEvent, MoyasarError> {
    let verifier = ConstantTimeSecretVerifier::from_secret(secret)?;
    let event = verifier
        .verify_and_construct(raw_body)
        .map_err(|_| MoyasarError::InvalidWebhookSecret)?;
    Ok(map_unified_webhook_event(&event))
}

#[cfg(feature = "env-keys")]
fn resolve_signing_secret(secret: &str) -> Option<zeroize::Zeroizing<String>> {
    let trimmed = secret.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with('$') {
        let resolved = crate::kernel::env::resolve_env_secret(trimmed).ok()?;
        let inner = resolved.trim();
        if inner.is_empty() || inner.starts_with('$') {
            return None;
        }
        return Some(zeroize::Zeroizing::new(resolved));
    }
    Some(zeroize::Zeroizing::new(secret.to_owned()))
}

#[cfg(not(feature = "env-keys"))]
fn resolve_signing_secret(secret: &str) -> Option<zeroize::Zeroizing<String>> {
    let trimmed = secret.trim();
    if trimmed.is_empty() || trimmed.starts_with('$') {
        return None;
    }
    Some(zeroize::Zeroizing::new(secret.to_owned()))
}

fn compute_hmac_sha256_hex(payload: &[u8], key_bytes: &[u8]) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(key_bytes).expect("HMAC-SHA256 accepts keys of any size");
    mac.update(payload);
    let bytes = mac.finalize().into_bytes();
    let mut hex = String::with_capacity(64);
    for b in bytes {
        let _ = write!(&mut hex, "{b:02x}");
    }
    hex
}

/// Fallibly computes the lowercase hex-encoded HMAC-SHA256 signature of `payload` keyed by
/// `secret` (resolving `"$VAR_NAME"` or `"${VAR_NAME}"` when `env-keys` is enabled).
///
/// Fails closed with [`MoyasarError::InvalidWebhookSecret`] if `secret` is empty or refers to an
/// unresolved environment variable.
///
/// # Errors
///
/// Returns [`MoyasarError::InvalidWebhookSecret`] when `secret` cannot be resolved to a non-empty
/// signing key.
pub fn try_compute_webhook_signature(payload: &[u8], secret: &str) -> Result<String, MoyasarError> {
    let resolved = resolve_signing_secret(secret).ok_or(MoyasarError::InvalidWebhookSecret)?;
    Ok(compute_hmac_sha256_hex(payload, resolved.as_bytes()))
}

/// Computes the lowercase hex-encoded HMAC-SHA256 signature of `payload` keyed by `secret`
/// (resolving `"$VAR_NAME"` or `"${VAR_NAME}"` behind the scenes when `env-keys` is enabled).
///
/// Returns an empty string instead of signing with literal `"$VAR"` bytes when `secret` is empty
/// or unresolved. Prefer [`try_compute_webhook_signature`] for explicit error handling.
#[must_use]
pub fn compute_webhook_signature(payload: &[u8], secret: &str) -> String {
    try_compute_webhook_signature(payload, secret).unwrap_or_default()
}

/// Verifies a hex-encoded HMAC-SHA256 `signature_header` (with optional `sha256=` prefix)
/// against `payload` and `secret` in constant time.
///
/// # Errors
///
/// Returns [`MoyasarError::InvalidWebhookSignature`] if the signature is malformed or does not match.
pub fn verify_webhook_signature(
    payload: &[u8],
    signature_header: &str,
    secret: &str,
) -> Result<(), MoyasarError> {
    let Some(resolved) = resolve_signing_secret(secret) else {
        return Err(MoyasarError::InvalidWebhookSignature);
    };
    let trimmed = signature_header.trim();
    let candidate = trimmed
        .strip_prefix("sha256=")
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    let mut expected = compute_hmac_sha256_hex(payload, resolved.as_bytes());
    let matched = candidate.as_bytes().ct_eq(expected.as_bytes()).unwrap_u8() == 1;
    expected.zeroize();
    if !matched {
        return Err(MoyasarError::InvalidWebhookSignature);
    }
    Ok(())
}

fn verify_static_secret_token(token: &str, secret: &str) -> Result<(), MoyasarError> {
    let resolved = resolve_signing_secret(secret).ok_or(MoyasarError::InvalidWebhookSecret)?;
    let mut provided = ConstantTimeSecretVerifier::compute_digest(token.as_bytes());
    let mut expected = ConstantTimeSecretVerifier::compute_digest(resolved.as_bytes());
    let matched = provided.ct_eq(&expected).unwrap_u8() == 1;
    provided.zeroize();
    expected.zeroize();
    if !matched {
        return Err(MoyasarError::InvalidWebhookSecret);
    }
    Ok(())
}

fn header_str<'a>(headers: &'a http::HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|val| val.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn resolved_webhook_secret(client: &MoyasarClient<SecretKey>) -> Result<&str, MoyasarError> {
    client
        .webhook_secret()
        .map(ExposeSecret::expose_secret)
        .filter(|s| !s.trim().is_empty())
        .ok_or(MoyasarError::InvalidWebhookSecret)
}

fn map_unified_webhook_event(event: &WebhookEvent) -> UnifiedWebhookEvent {
    UnifiedWebhookEvent {
        id: event.id,
        event_type: event.event_type.as_str().to_owned(),
        created_at: event.created_at,
        live: event.live,
        data: event.data.clone(),
    }
}

async fn execute_unified_reversal(
    client: &MoyasarClient<SecretKey>,
    transaction_id: &str,
    amount_minor: Option<u64>,
) -> Result<UnifiedReversalResponse, MoyasarError> {
    let id = PaymentId::new(transaction_id)?;
    let payments = client.payments();
    let payment = payments.retrieve(&id).await?;
    validate_refund_amount(&payment, amount_minor)?;

    if should_void_unified(&payment, amount_minor) {
        match payments.void(id.as_str()).await {
            Ok(voided) => {
                return Ok(build_reversal_response(voided, "voided", payment.amount));
            }
            Err(err) if !is_void_fallback_eligible(&payment, &err) => return Err(err),
            Err(_) => {}
        }
    }
    let delta = amount_minor.unwrap_or_else(|| remaining_refundable_amount(&payment));
    let refunded = payments.refund_payment(&payment, amount_minor).await?;
    Ok(build_reversal_response(refunded, "refunded", delta))
}

fn should_void_unified(payment: &Payment, amount_minor: Option<u64>) -> bool {
    let is_full = payment.refunded == 0 && amount_minor.is_none_or(|amt| amt == payment.amount);
    is_full && can_void_payment(payment, Utc::now())
}

fn build_reversal_response(
    payment: Payment,
    action: &str,
    amount_reversed_minor: u64,
) -> UnifiedReversalResponse {
    UnifiedReversalResponse {
        transaction_id: payment.id,
        action: action.to_owned(),
        amount_reversed_minor,
        currency: payment.currency,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::{Value, json};
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn unified_payment_json(status: &str, now: &str) -> Value {
        json!({
            "id": "pay_01J9UNIFIED",
            "status": status,
            "amount": 15000,
            "amount_format": "150.00 SAR",
            "fee": 0,
            "fee_format": "0.00 SAR",
            "refunded": 0,
            "refunded_format": "0.00 SAR",
            "captured": if status == "voided" { 0 } else { 15000 },
            "captured_format": if status == "voided" { "0.00 SAR" } else { "150.00 SAR" },
            "currency": "SAR",
            "source": {"type": "token"},
            "voided_at": (status == "voided").then_some(now),
            "created_at": now,
            "updated_at": now
        })
    }

    async fn mount_unified_mocks(server: &MockServer, now: &str) {
        Mock::given(method("POST"))
            .and(path("/v1/payment_intents"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "pi_01J9UNIFIED",
                "status": "created",
                "amount": 15000,
                "amount_format": "150.00 SAR",
                "currency": "SAR",
                "client_secret": "pi_secret_123",
                "expires_at": now,
                "created_at": now,
                "updated_at": now
            })))
            .mount(server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/tokens/token_abc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "token_abc",
                "status": "active",
                "number": "411111XXXXXX1111",
                "company": "visa",
                "gateway_id": "gw_1",
                "created_at": now
            })))
            .mount(server)
            .await;

        let paid = unified_payment_json("paid", now);
        Mock::given(method("POST"))
            .and(path("/v1/payments"))
            .respond_with(ResponseTemplate::new(201).set_body_json(paid.clone()))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/payments/pay_01J9UNIFIED"))
            .respond_with(ResponseTemplate::new(200).set_body_json(paid))
            .mount(server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/payments/pay_01J9UNIFIED/void"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(unified_payment_json("voided", now)),
            )
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn tc_8_3_0_01_unified_gateway_dispatches_all_four_operations() {
        let server = MockServer::start().await;
        let now = Utc::now().to_rfc3339();
        mount_unified_mocks(&server, &now).await;

        let client = MoyasarClient::builder(SecretKey::new("sk_test_unified").unwrap())
            .base_url(server.uri())
            .webhook_secret("whsec_test_123")
            .build()
            .unwrap();

        let intent_req = UnifiedIntentRequest::new(15000, "SAR", "https://example.com/cb")
            .description("Order #1");
        let intent = client.create_intent(intent_req).await.unwrap();
        assert_eq!(intent.amount_minor, 15000);

        let charge_req = UnifiedTokenChargeRequest::new(15000, "SAR", "token_abc", Uuid::new_v4());
        let charge = client.charge_saved_token(charge_req).await.unwrap();
        assert_eq!(charge.transaction_id, "pay_01J9UNIFIED");

        let reversal = client
            .refund_or_void("pay_01J9UNIFIED", None)
            .await
            .unwrap();
        assert_eq!(reversal.action, "voided");
        assert_eq!(reversal.amount_reversed_minor, 15000);
        assert_eq!(reversal.currency, Currency::SAR);

        let payload = json!({
            "id": Uuid::new_v4(),
            "type": "payment_paid",
            "created_at": now,
            "secret_token": "whsec_test_123",
            "live": false,
            "data": {"id": "pay_01J9UNIFIED"}
        });
        let raw_bytes = payload.to_string().into_bytes();
        let sig = compute_webhook_signature(&raw_bytes, "whsec_test_123");
        let mut headers = http::HeaderMap::new();
        headers.insert(WEBHOOK_SIGNATURE_HEADER, sig.parse().unwrap());
        let evt = client.verify_webhook(&headers, &raw_bytes).unwrap();
        assert_eq!(evt.event_type, "payment_paid");
    }

    #[test]
    fn verify_webhook_requires_hmac_signature_while_opt_in_accepts_secret_token() {
        let client = MoyasarClient::builder(SecretKey::new("sk_test_unified").unwrap())
            .webhook_secret("whsec_real_secret")
            .build()
            .unwrap();
        let now = Utc::now().to_rfc3339();
        let payload = json!({
            "id": Uuid::new_v4(),
            "type": "payment_paid",
            "created_at": now,
            "secret_token": "whsec_real_secret",
            "live": false,
            "data": {"id": "pay_01J9UNIFIED"}
        });
        let raw_bytes = payload.to_string().into_bytes();

        let mut token_only_headers = http::HeaderMap::new();
        token_only_headers.insert(
            WEBHOOK_SECRET_TOKEN_HEADER,
            "whsec_real_secret".parse().unwrap(),
        );

        let err = client
            .verify_webhook(&token_only_headers, &raw_bytes)
            .expect_err("verify_webhook must reject missing x-moyasar-signature");
        assert!(matches!(err, MoyasarError::InvalidWebhookSignature));

        let evt = client
            .verify_webhook_with_secret_token(&token_only_headers, &raw_bytes)
            .expect("explicit opt-in accepts matching x-moyasar-secret-token");
        assert_eq!(evt.event_type, "payment_paid");

        let mut bad_token_headers = http::HeaderMap::new();
        bad_token_headers.insert(WEBHOOK_SECRET_TOKEN_HEADER, "forged".parse().unwrap());
        let bad_err = client
            .verify_webhook_with_secret_token(&bad_token_headers, &raw_bytes)
            .expect_err("mismatched x-moyasar-secret-token must fail");
        assert!(matches!(bad_err, MoyasarError::InvalidWebhookSecret));
    }

    #[test]
    fn sec_01_webhook_verification_fails_closed_without_webhook_secret() {
        let api_key = "sk_test_never_use_as_webhook_secret";
        let client = MoyasarClient::new(SecretKey::new(api_key).unwrap()).unwrap();
        let now = Utc::now().to_rfc3339();
        let payload = json!({
            "id": Uuid::new_v4(),
            "type": "payment_paid",
            "created_at": now,
            "secret_token": api_key,
            "live": false,
            "data": {"id": "pay_sec01"}
        });
        let raw_bytes = payload.to_string().into_bytes();
        let sig = compute_webhook_signature(&raw_bytes, api_key);
        let mut headers = http::HeaderMap::new();
        headers.insert(WEBHOOK_SIGNATURE_HEADER, sig.parse().unwrap());

        assert!(matches!(
            client.verify_webhook(&headers, &raw_bytes),
            Err(MoyasarError::InvalidWebhookSecret)
        ));
        assert!(matches!(
            client.verify_webhook_payload(&raw_bytes),
            Err(MoyasarError::InvalidWebhookSecret)
        ));

        let mut token_headers = http::HeaderMap::new();
        token_headers.insert(WEBHOOK_SECRET_TOKEN_HEADER, api_key.parse().unwrap());
        assert!(matches!(
            client.verify_webhook_with_secret_token(&token_headers, &raw_bytes),
            Err(MoyasarError::InvalidWebhookSecret)
        ));

        assert!(matches!(
            try_compute_webhook_signature(&raw_bytes, "$UNRESOLVED_WEBHOOK_SECRET_XYZ"),
            Err(MoyasarError::InvalidWebhookSecret)
        ));
        assert!(compute_webhook_signature(&raw_bytes, "$UNRESOLVED_WEBHOOK_SECRET_XYZ").is_empty());
    }

    #[test]
    fn verify_and_normalize_supports_native_moyasar_payload_without_hmac_header() {
        let client = MoyasarClient::builder(SecretKey::new("sk_test_native_wh").unwrap())
            .webhook_secret("whsec_native_secret")
            .build()
            .unwrap();
        let now = Utc::now().to_rfc3339();
        let payload = json!({
            "id": Uuid::new_v4(),
            "type": "payment_paid",
            "created_at": now,
            "secret_token": "whsec_native_secret",
            "live": false,
            "data": {"id": "pay_native_01"}
        });
        let raw_bytes = payload.to_string().into_bytes();

        let empty_headers = http::HeaderMap::new();
        let evt = client
            .verify_and_normalize(&empty_headers, &raw_bytes)
            .expect("native Moyasar secret_token payload should verify");
        assert_eq!(evt.event_type, "payment_paid");

        let direct = client
            .verify_webhook_payload(&raw_bytes)
            .expect("verify_webhook_payload should verify native payload");
        assert_eq!(direct.id, evt.id);
    }

    #[tokio::test]
    async fn rich_intent_request_forwards_metadata_and_idempotency_key() {
        let server = MockServer::start().await;
        let now = Utc::now().to_rfc3339();
        let idem = Uuid::new_v4();

        Mock::given(method("POST"))
            .and(path("/v1/payment_intents"))
            .and(body_partial_json(json!({
                "amount": 25000,
                "currency": "SAR",
                "given_id": idem,
                "metadata": {
                    "order_reference": "ORD-9001",
                    "buyer_country": "SA",
                    "customer_email": "buyer@example.com",
                    "customer_phone": "+966512345678"
                }
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "pi_rich_01",
                "status": "created",
                "amount": 25000,
                "amount_format": "250.00 SAR",
                "currency": "SAR",
                "client_secret": "pi_secret_rich",
                "expires_at": now,
                "created_at": now,
                "updated_at": now
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = MoyasarClient::builder(SecretKey::new("sk_test_rich_intent").unwrap())
            .base_url(server.uri())
            .build()
            .unwrap();

        let req = UnifiedIntentRequest::new(25_000, "SAR", "https://example.com/cb")
            .description("Rich Order")
            .order_reference("ORD-9001")
            .buyer_country("SA")
            .customer_email("buyer@example.com")
            .customer_phone("+966512345678")
            .idempotency_key(idem);

        let intent = client.create_intent(req).await.unwrap();
        assert_eq!(intent.id, "pi_rich_01");
        assert_eq!(intent.amount_minor, 25_000);
    }
}
