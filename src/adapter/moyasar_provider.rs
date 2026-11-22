//! Optional `some-payment-core::PaymentProvider` implementation for Moyasar (US-9.2.0 Unit 3).
//!
//! Bridges `some-moyasar`'s offline alignment guards, pre-flight validators, error classifier,
//! and constant-time webhook scrubber into the cross-gateway
//! [`some_payment_core::PaymentProvider`] SPI.

use core::fmt;

use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use some_payment_core::{
    CaptureRequest, CheckoutSessionRequest, CreatePaymentRequest, Money, NormalizedWebhookEvent,
    PaymentCapabilities, PaymentError, PaymentProvider, PaymentResult, ProviderId,
    ProviderReference, RawWebhookRequest, RefundRequest, RefundResult, VoidRequest,
};

use crate::alignment::idempotency_errors::validate_given_id;
use crate::alignment::key_scope::{SecretOnlyOperation, detect_key_mode, require_secret_key};
use crate::alignment::preflight_rules::{
    build_mutation_body, validate_metadata_limits, validate_partial_amount,
};
use crate::alignment::webhook_token_verifier::verify_and_scrub_moyasar_webhook;

pub use super::config::MoyasarProviderConfig;
use super::mapper;

const MOYASAR_PROVIDER_SLUG: &str = "moyasar";

struct MutationSpec<'a> {
    op: SecretOnlyOperation,
    reference: &'a ProviderReference,
    action: &'static str,
    amount: Option<&'a Money>,
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
        let provider = Self {
            provider_id: ProviderId::new(MOYASAR_PROVIDER_SLUG),
            api_key: config.api_key,
            webhook_secret: config.webhook_secret,
            base_url: config.base_url,
            http_client: config.http_client,
            livemode: classification.mode.is_live(),
            allow_raw_cards: config.allow_raw_cards,
        };
        if provider.base_url().is_empty() || provider.capabilities().is_empty() {
            return Err(PaymentError::configuration("base_url must not be empty"));
        }
        Ok(provider)
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
            .map_err(|err| mapper::map_local_error(self.id(), err))
    }

    fn parse_payment_result(
        &self,
        payload: &Value,
        order_id: Option<&str>,
    ) -> Result<PaymentResult, PaymentError> {
        mapper::parse_payment_result(self.id(), self.livemode(), payload, order_id)
    }

    async fn post_payment_mutation(&self, spec: MutationSpec<'_>) -> Result<Value, PaymentError> {
        self.require_secret(spec.op)?;
        let payment_uuid = mapper::extract_payment_uuid(self.id(), spec.reference)?;
        let partial = validate_partial_amount(spec.amount.map(Money::amount))
            .map_err(|err| mapper::map_local_error(self.id(), err))?;
        let body = build_mutation_body(partial);
        let path = format!("/payments/{payment_uuid}/{}", spec.action);
        self.send_post(&path, body.as_ref()).await
    }

    async fn send_request(&self, req: reqwest::RequestBuilder) -> Result<Value, PaymentError> {
        let resp = req
            .basic_auth(self.api_key.expose_secret(), Some(""))
            .send()
            .await
            .map_err(|err| mapper::map_transport_error(self.id(), &err))?;
        self.parse_json_response(resp).await
    }

    async fn send_get(&self, path: &str) -> Result<Value, PaymentError> {
        let url = format!("{}{path}", self.base_url());
        self.send_request(self.http_client.get(url)).await
    }

    async fn send_post(&self, path: &str, body: Option<&Value>) -> Result<Value, PaymentError> {
        let url = format!("{}{path}", self.base_url());
        let mut builder = self.http_client.post(url);
        if let Some(payload) = body {
            builder = builder.json(payload);
        }
        self.send_request(builder).await
    }

    async fn parse_json_response(&self, resp: reqwest::Response) -> Result<Value, PaymentError> {
        let status = resp.status();
        let retry_after = resp
            .headers()
            .get("retry-after")
            .and_then(|val| val.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok());
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(mapper::map_api_error(
                self.id(),
                status.as_u16(),
                &body,
                retry_after,
            ));
        }
        serde_json::from_str(&body).map_err(|_| PaymentError::ProviderError {
            provider: self.id().clone(),
            http_status: Some(status.as_u16()),
            vendor_code: None,
            message: "invalid JSON response from Moyasar".to_owned(),
        })
    }
}

impl fmt::Debug for MoyasarProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MoyasarProvider")
            .field("provider_id", &self.id())
            .field("api_key", &"[REDACTED]")
            .field(
                "webhook_secret",
                &self.webhook_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .field("base_url", &self.base_url())
            .field("livemode", &self.livemode())
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
        let source_type = mapper::resolve_source_type(req);
        mapper::validate_create_guards(self.id(), self.allow_raw_cards, req, source_type)?;
        let given_id = req
            .idempotency_key
            .as_deref()
            .map(|raw| {
                validate_given_id(raw).map_err(|err| mapper::map_local_error(self.id(), err))
            })
            .transpose()?;
        let body = mapper::build_create_payment_body(req, source_type, given_id);
        let payload = self.send_post("/payments", Some(&body)).await?;
        self.parse_payment_result(&payload, Some(&req.order_id))
    }

    async fn get_payment(
        &self,
        reference: &ProviderReference,
    ) -> Result<PaymentResult, PaymentError> {
        self.require_secret(SecretOnlyOperation::RetrievePayment)?;
        let payment_uuid = mapper::extract_payment_uuid(self.id(), reference)?;
        let payload = self.send_get(&format!("/payments/{payment_uuid}")).await?;
        self.parse_payment_result(&payload, reference.order_id())
    }

    async fn create_checkout(
        &self,
        req: &CheckoutSessionRequest,
    ) -> Result<PaymentResult, PaymentError> {
        self.require_secret(SecretOnlyOperation::CreateInvoice)?;
        req.validate()?;
        validate_metadata_limits(&req.metadata)
            .map_err(|err| mapper::map_local_error(self.id(), err))?;
        let body = mapper::build_checkout_body(req);
        let payload = self.send_post("/invoices", Some(&body)).await?;
        mapper::parse_invoice_result(self.id(), self.livemode(), &payload, &req.order_id)
    }

    async fn capture(&self, req: &CaptureRequest) -> Result<PaymentResult, PaymentError> {
        req.validate()?;
        let payload = self
            .post_payment_mutation(MutationSpec {
                op: SecretOnlyOperation::Capture,
                reference: &req.reference,
                action: "capture",
                amount: req.amount.as_ref(),
            })
            .await?;
        self.parse_payment_result(&payload, req.reference.order_id())
    }

    async fn void_payment(&self, req: &VoidRequest) -> Result<PaymentResult, PaymentError> {
        req.validate()?;
        let payload = self
            .post_payment_mutation(MutationSpec {
                op: SecretOnlyOperation::Void,
                reference: &req.reference,
                action: "void",
                amount: None,
            })
            .await?;
        self.parse_payment_result(&payload, req.reference.order_id())
    }

    async fn refund(&self, req: &RefundRequest) -> Result<RefundResult, PaymentError> {
        req.validate()?;
        let payload = self
            .post_payment_mutation(MutationSpec {
                op: SecretOnlyOperation::Refund,
                reference: &req.reference,
                action: "refund",
                amount: req.amount.as_ref(),
            })
            .await?;
        mapper::parse_refund_result(self.id(), &payload, req)
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
            .map_err(|err| mapper::map_local_error(self.id(), err))?;
        mapper::map_webhook_event(self.id(), verified)
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

    fn sample_reference() -> ProviderReference {
        ProviderReference::builder(ProviderId::new("moyasar"))
            .payment_id(VALID_UUID)
            .build()
            .expect("valid reference")
    }

    fn raw_webhook(body: &str) -> RawWebhookRequest {
        RawWebhookRequest {
            raw_body: body.as_bytes().to_vec(),
            headers: BTreeMap::new(),
            query_params: BTreeMap::new(),
            method: HttpMethod::Post,
        }
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
            .with_webhook_secret("wh_canary_secret_888")
            .with_http_client(reqwest::Client::new())
            .with_allow_raw_cards(false);
        let provider = MoyasarProvider::new(cfg).expect("valid");
        assert!(!provider.base_url().is_empty());
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

        let faild_body = json!({
            "id": "evt_typo_1",
            "type": "payment_faild",
            "live": false,
            "secret_token": "wh_secret_canary_777",
            "data": { "id": VALID_UUID, "amount": 2500, "currency": "SAR" }
        })
        .to_string();
        let raw_faild = raw_webhook(&faild_body);
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
        .to_string();
        let raw_hs = raw_webhook(&handshake_body);
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
