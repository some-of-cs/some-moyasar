//! Unified cross-gateway payment trait adapter for `MoyasarClient`.

use std::future::Future;
use std::pin::Pin;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kernel::auth::{AuthKey, SecretKey};
use crate::kernel::client::MoyasarClient;
use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::types::Currency;
use crate::resources::payment_intents::CreatePaymentIntentParams;
use crate::resources::payments::{
    CreatePaymentParams, Payment, PaymentId, PaymentSourceRequest, TokenSource, can_void_payment,
};
use crate::webhook_engine::{ConstantTimeSecretVerifier, REPLAY_TOLERANCE};

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
    /// Reversed amount in minor units (`u64`).
    pub amount_reversed_minor: u64,
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
        amount_minor: u64,
        currency: &'a str,
        description: Option<&'a str>,
        callback_url: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedIntentResponse, Self::Error>> + Send + 'a>>;

    /// Charges a saved token off-session with explicit idempotency key normalization.
    fn charge_saved_token<'a>(
        &'a self,
        amount_minor: u64,
        currency: &'a str,
        token_id: &'a str,
        idempotency_key: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedChargeResponse, Self::Error>> + Send + 'a>>;

    /// Reverses a transaction via smart void (when full & unsettled) or refund.
    fn refund_or_void<'a>(
        &'a self,
        transaction_id: &'a str,
        amount_minor: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedReversalResponse, Self::Error>> + Send + 'a>>;

    /// Verifies and parses an inbound webhook payload using constant-time comparison.
    fn verify_webhook(
        &self,
        headers: &http::HeaderMap,
        raw_body: &[u8],
    ) -> Result<UnifiedWebhookEvent, Self::Error>;
}

impl UnifiedPaymentGateway for MoyasarClient<SecretKey> {
    type Error = MoyasarError;

    fn create_intent<'a>(
        &'a self,
        amount_minor: u64,
        currency: &'a str,
        description: Option<&'a str>,
        callback_url: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedIntentResponse, Self::Error>> + Send + 'a>> {
        Box::pin(async move {
            let cur: Currency = currency.parse()?;
            let mut params = CreatePaymentIntentParams::new(amount_minor, cur, callback_url);
            if let Some(desc) = description {
                params = params.description(desc);
            }
            let intent = self.payment_intents().create(params).await?;
            let client_secret = intent.client_secret().map(str::to_owned);
            Ok(UnifiedIntentResponse {
                id: intent.id.as_str().to_owned(),
                client_secret,
                amount_minor: intent.amount,
                currency: intent.currency,
                expires_at: intent.expires_at,
            })
        })
    }

    fn charge_saved_token<'a>(
        &'a self,
        amount_minor: u64,
        currency: &'a str,
        token_id: &'a str,
        idempotency_key: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedChargeResponse, Self::Error>> + Send + 'a>> {
        Box::pin(async move {
            let cur: Currency = currency.parse()?;
            let mut source = TokenSource::new(token_id)?;
            source.three_ds = Some(false);
            let params = CreatePaymentParams::builder()
                .amount(amount_minor)
                .currency(cur)
                .given_id(idempotency_key)
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
        Box::pin(async move {
            let id = PaymentId::new(transaction_id)?;
            let payment = self.payments().retrieve(&id).await?;
            if should_void_unified(&payment, amount_minor) {
                return void_unified_payment(self, &id).await;
            }
            refund_unified_payment(self, &id, amount_minor).await
        })
    }

    fn verify_webhook(
        &self,
        headers: &http::HeaderMap,
        raw_body: &[u8],
    ) -> Result<UnifiedWebhookEvent, Self::Error> {
        let secret = headers
            .get("x-moyasar-webhook-secret")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_else(|| self.key().expose_secret());
        let verifier = ConstantTimeSecretVerifier::new(secret, REPLAY_TOLERANCE);
        let event = verifier
            .verify_and_construct(raw_body)
            .map_err(|_| MoyasarError::Api {
                error_type: ApiErrorType::AuthenticationError,
                message: "webhook verification failed".to_owned(),
                errors: Vec::new(),
                response_code: None,
            })?;
        Ok(UnifiedWebhookEvent {
            id: event.id,
            event_type: event.event_type.as_str().to_owned(),
            created_at: event.created_at,
            live: event.live,
            data: event.data.clone(),
        })
    }
}

fn should_void_unified(payment: &Payment, amount_minor: Option<u64>) -> bool {
    let is_full = amount_minor.is_none_or(|amt| amt == payment.amount);
    is_full && can_void_payment(payment, Utc::now())
}

async fn void_unified_payment(
    client: &MoyasarClient<SecretKey>,
    id: &PaymentId,
) -> Result<UnifiedReversalResponse, MoyasarError> {
    let voided = client.payments().void(id.as_str()).await?;
    Ok(UnifiedReversalResponse {
        transaction_id: voided.id,
        action: "voided".to_owned(),
        amount_reversed_minor: voided.amount,
    })
}

async fn refund_unified_payment(
    client: &MoyasarClient<SecretKey>,
    id: &PaymentId,
    amount_minor: Option<u64>,
) -> Result<UnifiedReversalResponse, MoyasarError> {
    let refunded = client.payments().refund(id.as_str(), amount_minor).await?;
    Ok(UnifiedReversalResponse {
        transaction_id: refunded.id,
        action: "refunded".to_owned(),
        amount_reversed_minor: refunded.refunded,
    })
}


#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[tokio::test]
    async fn tc_8_3_0_01_unified_gateway_dispatches_all_four_operations() {
        let server = MockServer::start().await;
        let now = Utc::now().to_rfc3339();
        let given_id = Uuid::new_v4();

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
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/payments"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "pay_01J9UNIFIED",
                "status": "paid",
                "amount": 15000,
                "amount_format": "150.00 SAR",
                "fee": 0,
                "fee_format": "0.00 SAR",
                "refunded": 0,
                "refunded_format": "0.00 SAR",
                "captured": 15000,
                "captured_format": "150.00 SAR",
                "currency": "SAR",
                "source": {"type": "token"},
                "created_at": now,
                "updated_at": now
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/payments/pay_01J9UNIFIED"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "pay_01J9UNIFIED",
                "status": "paid",
                "amount": 15000,
                "amount_format": "150.00 SAR",
                "fee": 0,
                "fee_format": "0.00 SAR",
                "refunded": 0,
                "refunded_format": "0.00 SAR",
                "captured": 15000,
                "captured_format": "150.00 SAR",
                "currency": "SAR",
                "source": {"type": "token"},
                "created_at": now,
                "updated_at": now
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/payments/pay_01J9UNIFIED/void"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "pay_01J9UNIFIED",
                "status": "voided",
                "amount": 15000,
                "amount_format": "150.00 SAR",
                "fee": 0,
                "fee_format": "0.00 SAR",
                "refunded": 0,
                "refunded_format": "0.00 SAR",
                "captured": 0,
                "captured_format": "0.00 SAR",
                "currency": "SAR",
                "source": {"type": "token"},
                "voided_at": now,
                "created_at": now,
                "updated_at": now
            })))
            .mount(&server)
            .await;

        let client = MoyasarClient::builder(SecretKey::new("sk_test_unified").unwrap())
            .base_url(server.uri())
            .build()
            .unwrap();

        let intent = client
            .create_intent(15000, "SAR", Some("Order #1"), "https://example.com/cb")
            .await
            .unwrap();
        assert_eq!(intent.amount_minor, 15000);

        let charge = client
            .charge_saved_token(15000, "SAR", "token_abc", given_id)
            .await
            .unwrap();
        assert_eq!(charge.transaction_id, "pay_01J9UNIFIED");

        let reversal = client
            .refund_or_void("pay_01J9UNIFIED", None)
            .await
            .unwrap();
        assert_eq!(reversal.action, "voided");
        assert_eq!(reversal.amount_reversed_minor, 15000);

        let payload = json!({
            "id": Uuid::new_v4(),
            "type": "payment_paid",
            "created_at": now,
            "secret_token": "whsec_test_123",
            "live": false,
            "data": {"id": "pay_01J9UNIFIED"}
        });
        let mut headers = http::HeaderMap::new();
        headers.insert(
            "x-moyasar-webhook-secret",
            "whsec_test_123".parse().unwrap(),
        );
        let evt = client
            .verify_webhook(&headers, payload.to_string().as_bytes())
            .unwrap();
        assert_eq!(evt.event_type, "payment_paid");
    }
}
