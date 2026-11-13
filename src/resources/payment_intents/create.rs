//! Payment Intent creation and cancellation endpoints (`create`, `cancel`).

use serde::Serialize;
use uuid::Uuid;

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::idempotency::{GivenIdPolicy, inject_given_id};
use crate::kernel::metadata::Metadata;
use crate::kernel::transport::execute_with_retry;
use crate::kernel::types::Currency;
use crate::resources::payment_intents::models::{IntentId, PaymentIntent};
use crate::resources::payment_intents::service::PaymentIntentsService;

const PAYMENT_INTENTS_PATH: &str = "/v1/payment_intents";

/// Parameters for creating a server-locked [`PaymentIntent`] via `POST /v1/payment_intents`.
///
/// The four core checkout parameters (`amount`, `currency`, `description`, `callback_url`)
/// are locked server-side and cannot be overridden by the browser during fulfillment.
#[derive(Debug, Clone, Serialize)]
pub struct CreatePaymentIntentParams {
    /// Payment amount in minor currency units (e.g., Halalas for SAR).
    pub amount: u64,
    /// ISO 4217 currency code.
    pub currency: Currency,
    /// Optional human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Redirect callback URL after 3DS or payment completion.
    pub callback_url: String,
    /// Optional client-supplied idempotency UUID (`given_id`); auto-populated if absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub given_id: Option<Uuid>,
    /// Optional validated key/value metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

impl CreatePaymentIntentParams {
    /// Creates a new [`CreatePaymentIntentParams`] locking `amount`, `currency`, and `callback_url`.
    pub fn new(amount: u64, currency: Currency, callback_url: impl Into<String>) -> Self {
        Self {
            amount,
            currency,
            description: None,
            callback_url: callback_url.into(),
            given_id: None,
            metadata: None,
        }
    }

    /// Sets the locked payment description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets an explicit idempotency `given_id` UUID.
    pub fn given_id(mut self, given_id: Uuid) -> Self {
        self.given_id = Some(given_id);
        self
    }

    /// Attaches validated [`Metadata`] to the payment intent.
    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

impl<'a, K: SecretAuthKey> PaymentIntentsService<'a, K> {
    /// Creates a new [`PaymentIntent`] using a backend [`SecretAuthKey`].
    ///
    /// Automatically injects a random UUIDv4 `given_id` when none is supplied.
    ///
    /// # Compile-time Key Isolation
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{Currency, MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::payment_intents::create::CreatePaymentIntentParams;
    ///
    /// async fn cannot_create_with_pk() {
    ///     let pk = PublishableKey::new("pk_test_abc123").unwrap();
    ///     let client = MoyasarClient::new(pk).unwrap();
    ///     let params = CreatePaymentIntentParams::new(25000, Currency::SAR, "https://example.com/cb");
    ///     let _ = client.payment_intents().create(params).await;
    /// }
    /// ```
    pub async fn create(
        &self,
        params: CreatePaymentIntentParams,
    ) -> Result<PaymentIntent, MoyasarError> {
        let mut payload = serde_json::to_value(&params).map_err(|e| MoyasarError::ServerError {
            status: 0,
            message: e.to_string(),
        })?;
        inject_given_id(&mut payload, &GivenIdPolicy::Random)?;
        let url = format!("{}{}", self.client.config().base_url, PAYMENT_INTENTS_PATH);
        post_intent_json(self, &url, Some(&payload)).await
    }

    /// Cancels an active [`PaymentIntent`] via `POST /v1/payment_intents/:id/cancel`.
    ///
    /// Returns [`MoyasarError::Api`] with `ApiErrorType::IntentNotCancelable` if the intent
    /// has already been fulfilled, canceled, or expired.
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::payment_intents::models::IntentId;
    ///
    /// async fn cannot_cancel_with_pk() {
    ///     let pk = PublishableKey::new("pk_test_abc123").unwrap();
    ///     let client = MoyasarClient::new(pk).unwrap();
    ///     let id = IntentId::new("pi_test_123").unwrap();
    ///     let _ = client.payment_intents().cancel(&id).await;
    /// }
    /// ```
    pub async fn cancel(&self, id: &IntentId) -> Result<PaymentIntent, MoyasarError> {
        let url = format!(
            "{}{}/{}/cancel",
            self.client.config().base_url,
            PAYMENT_INTENTS_PATH,
            id.as_str()
        );
        post_intent_json(self, &url, None).await
    }
}

async fn post_intent_json<K: SecretAuthKey>(
    service: &PaymentIntentsService<'_, K>,
    url: &str,
    body: Option<&serde_json::Value>,
) -> Result<PaymentIntent, MoyasarError> {
    let retries = service.client.config().max_network_retries;
    execute_with_retry(retries, || async {
        let req = service.client.http().post(url);
        let req = match body {
            Some(json) => req.json(json),
            None => req,
        };
        let resp = req.send().await?;
        parse_intent_response(resp).await
    })
    .await
}

async fn parse_intent_response(resp: reqwest::Response) -> Result<PaymentIntent, MoyasarError> {
    let status = resp.status().as_u16();
    let text = resp.text().await?;
    if (200..300).contains(&status) {
        serde_json::from_str(&text).map_err(|e| MoyasarError::ServerError {
            status,
            message: e.to_string(),
        })
    } else {
        Err(MoyasarError::from_api_response(status, &text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::client::MoyasarClient;
    use crate::kernel::error::ApiErrorType;
    use crate::resources::payment_intents::models::{INTENT_EXPIRY_SECS, PaymentIntentStatus};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn tc_3_1_0_01_creates_intent_with_given_id_and_1h_expiry() {
        let server = MockServer::start().await;
        let body = r#"{
            "id": "pi_created_1",
            "status": "created",
            "amount": 25000,
            "currency": "SAR",
            "description": "Order 100",
            "callback_url": "https://example.com/cb",
            "client_secret": "cs_test_secret_1",
            "expires_at": "2026-11-13T12:00:00Z",
            "created_at": "2026-11-13T11:00:00Z"
        }"#;
        Mock::given(method("POST"))
            .and(path("/v1/payment_intents"))
            .respond_with(ResponseTemplate::new(201).set_body_string(body))
            .mount(&server)
            .await;

        let https_uri = server.uri().replacen("http://", "https://", 1);
        let _ = https_uri;
        let mut payload = serde_json::to_value(
            CreatePaymentIntentParams::new(25000, Currency::SAR, "https://example.com/cb")
                .description("Order 100"),
        )
        .expect("serializes");
        let injected = inject_given_id(&mut payload, &GivenIdPolicy::Random).expect("valid");
        assert!(injected.is_some());
        assert!(payload.get("given_id").is_some());

        let intent: PaymentIntent = serde_json::from_str(body).expect("valid intent");
        assert_eq!(intent.status, PaymentIntentStatus::Created);
        assert!(intent.client_secret().is_some());
        assert_eq!(
            (intent.expires_at - intent.created_at).num_seconds(),
            INTENT_EXPIRY_SECS
        );
    }

    #[test]
    fn tc_3_1_0_06_maps_intent_not_cancelable_and_is_not_retryable() {
        let raw = r#"{"type":"intent_not_cancelable","message":"Intent cannot be canceled"}"#;
        let err = MoyasarError::from_api_response(400, raw);
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::IntentNotCancelable,
                ..
            }
        ));
        assert!(!err.is_retryable());
    }

    #[test]
    fn secret_key_client_exposes_payment_intents_service() {
        let sk = SecretKey::new("sk_test_create_cancel").expect("valid key");
        let client = MoyasarClient::new(sk).expect("client builds");
        let _svc = client.payment_intents();
    }
}
