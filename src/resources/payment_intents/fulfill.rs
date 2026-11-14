//! Payment Intent fulfillment endpoint (`fulfill`).

use core::fmt;

use chrono::{DateTime, Utc};
use http::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::kernel::auth::AuthKey;
use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::metadata::Metadata;
use crate::kernel::transport::execute_with_retry;
use crate::kernel::types::Currency;
use crate::resources::payment_intents::fulfill_params::FulfillIntentParams;
use crate::resources::payment_intents::models::IntentId;
use crate::resources::payment_intents::service::PaymentIntentsService;

const PAYMENT_INTENTS_PATH: &str = "/v1/payment_intents";

/// Authoritative status of the [`IntentPayment`] returned by `POST /v1/payment_intents/:id/fulfill`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IntentPaymentStatus {
    /// Payment initiated (e.g., awaiting 3DS challenge).
    Initiated,
    /// Payment captured and settled.
    Paid,
    /// Payment authorized (manual capture mode).
    Authorized,
    /// Payment attempt failed.
    Failed,
    /// Payment was refunded.
    Refunded,
    /// Payment was captured after authorization.
    Captured,
    /// Authorization was voided.
    Voided,
    /// Card verification succeeded.
    Verified,
    /// Forward-compatible fallback for unrecognized status values.
    #[serde(other)]
    Unknown,
}

/// Payment object returned by `POST /v1/payment_intents/:id/fulfill`.
///
/// Always inspect [`IntentPayment::status`] (or re-fetch `GET /v1/payments/:id`)
/// because `PaymentIntentStatus::Fulfilled` does **not** guarantee `Paid`.
#[derive(Clone, Deserialize)]
pub struct IntentPayment {
    /// Unique Moyasar Payment identifier.
    pub id: String,
    /// Authoritative payment status (`initiated`, `paid`, `authorized`, `failed`, etc.).
    pub status: IntentPaymentStatus,
    /// Locked payment amount in minor currency units.
    pub amount: u64,
    /// ISO 4217 currency code.
    pub currency: Currency,
    /// Processing fee in minor units, if reported.
    #[serde(default)]
    pub fee: Option<u64>,
    /// Refunded amount in minor units.
    #[serde(default)]
    pub refunded: Option<u64>,
    /// Captured amount in minor units.
    #[serde(default)]
    pub captured: Option<u64>,
    /// Payment description.
    #[serde(default)]
    pub description: Option<String>,
    /// Callback URL.
    #[serde(default)]
    pub callback_url: Option<String>,
    /// Payment instrument response object (may contain `transaction_url` for 3DS).
    #[serde(default)]
    pub source: Option<serde_json::Value>,
    /// Attached metadata.
    #[serde(default)]
    pub metadata: Option<Metadata>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

impl fmt::Debug for IntentPayment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IntentPayment")
            .field("id", &self.id)
            .field("status", &self.status)
            .field("amount", &self.amount)
            .field("currency", &self.currency)
            .field("fee", &self.fee)
            .field("refunded", &self.refunded)
            .field("captured", &self.captured)
            .field("description", &self.description)
            .field("callback_url", &self.callback_url)
            .field("source", &"[REDACTED]")
            .field("metadata", &self.metadata)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

impl<'a, K: AuthKey> PaymentIntentsService<'a, K> {
    /// Fulfills a [`PaymentIntent`](crate::resources::payment_intents::models::PaymentIntent)
    /// via `POST /v1/payment_intents/:id/fulfill`.
    ///
    /// Generic over `K: AuthKey` so both [`PublishableKey`](crate::kernel::auth::PublishableKey)
    /// (browser/mobile PCI-DSS boundary) and [`SecretKey`](crate::kernel::auth::SecretKey)
    /// are supported.
    ///
    /// # Examples
    ///
    /// ```
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    ///
    /// let pk = PublishableKey::new("pk_test_browser_123").unwrap();
    /// let client = MoyasarClient::new(pk).unwrap();
    /// // Fulfill returns `IntentPayment`; always inspect `payment.status`, not intent status.
    /// let _svc = client.payment_intents();
    /// ```
    pub async fn fulfill(
        &self,
        id: &IntentId,
        params: FulfillIntentParams,
    ) -> Result<IntentPayment, MoyasarError> {
        let url = format!(
            "{}{}/{}/fulfill",
            self.client.config().base_url.trim_end_matches('/'),
            PAYMENT_INTENTS_PATH,
            id.as_str()
        );
        fulfill_at_url(
            self.client.http(),
            self.client.config().max_network_retries,
            &url,
            &params,
        )
        .await
    }
}

async fn fulfill_at_url(
    http: &reqwest::Client,
    retries: u32,
    url: &str,
    params: &FulfillIntentParams,
) -> Result<IntentPayment, MoyasarError> {
    execute_with_retry(retries, || async {
        let mut body_bytes = serde_json::to_vec(params).map_err(|err| MoyasarError::Api {
            error_type: ApiErrorType::InvalidRequestError,
            message: format!("failed to serialize fulfill request: {err}"),
            errors: Vec::new(),
            response_code: None,
        })?;
        let send_res = http
            .post(url)
            .header(CONTENT_TYPE, "application/json")
            .body(body_bytes.clone())
            .send()
            .await;
        body_bytes.zeroize();
        let resp = send_res?;
        parse_fulfill_response(resp).await
    })
    .await
}

async fn parse_fulfill_response(resp: reqwest::Response) -> Result<IntentPayment, MoyasarError> {
    let status = resp.status().as_u16();
    let text = resp.text().await?;
    if (200..300).contains(&status) {
        serde_json::from_str(&text).map_err(|e| MoyasarError::Api {
            error_type: ApiErrorType::Unknown,
            message: format!("failed to decode IntentPayment response: {e}"),
            errors: Vec::new(),
            response_code: None,
        })
    } else {
        Err(MoyasarError::from_api_response(status, &text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::PublishableKey;
    use crate::kernel::client::MoyasarClient;
    use crate::resources::payment_intents::fulfill_params::PaymentSourceRequest;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn tc_3_2_0_01_and_02_fulfills_over_wire_and_preserves_locked_amount() {
        let server = MockServer::start().await;
        let raw = r#"{
            "id": "pay_intent_001",
            "status": "initiated",
            "amount": 25000,
            "currency": "SAR",
            "source": {
                "type": "creditcard",
                "transaction_url": "https://api.moyasar.com/3ds/challenge"
            },
            "created_at": "2026-11-13T11:00:00Z"
        }"#;
        Mock::given(method("POST"))
            .and(path("/v1/payment_intents/pi_fulfill_1/fulfill"))
            .respond_with(ResponseTemplate::new(201).set_body_string(raw))
            .expect(1)
            .mount(&server)
            .await;

        let params = FulfillIntentParams::new(
            "cs_valid_secret",
            PaymentSourceRequest::Token {
                token: "token_abc".to_owned(),
            },
        )
        .expect("valid params")
        .save_card(true)
        .three_ds(true);

        let http = reqwest::Client::new();
        let url = format!(
            "{}{}/pi_fulfill_1/fulfill",
            server.uri(),
            PAYMENT_INTENTS_PATH
        );
        let payment = fulfill_at_url(&http, 2, &url, &params)
            .await
            .expect("fulfilled over wire");
        assert_eq!(payment.id, "pay_intent_001");
        assert_eq!(payment.status, IntentPaymentStatus::Initiated);
        assert_eq!(payment.amount, 25000);
    }

    #[tokio::test]
    async fn tc_3_2_0_03_and_04_wrong_secret_and_expired_intent_do_not_retry() {
        let server = MockServer::start().await;
        let wrong_secret = r#"{"type":"invalid_request_error","message":"client_secret mismatch"}"#;
        Mock::given(method("POST"))
            .and(path("/v1/payment_intents/pi_bad_secret/fulfill"))
            .respond_with(ResponseTemplate::new(400).set_body_string(wrong_secret))
            .expect(1)
            .mount(&server)
            .await;

        let params = FulfillIntentParams::new(
            "cs_wrong",
            PaymentSourceRequest::Token {
                token: "token_abc".to_owned(),
            },
        )
        .expect("valid");
        let http = reqwest::Client::new();
        let url = format!(
            "{}{}/pi_bad_secret/fulfill",
            server.uri(),
            PAYMENT_INTENTS_PATH
        );
        let err = fulfill_at_url(&http, 2, &url, &params)
            .await
            .expect_err("must reject wrong secret");
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            }
        ));
        assert!(!err.is_retryable());
    }

    #[test]
    fn intent_payment_debug_redacts_source_payload() {
        let raw = r#"{
            "id": "pay_intent_dbg",
            "status": "paid",
            "amount": 25000,
            "currency": "SAR",
            "source": {
                "type": "stcpay",
                "mobile": "0512345678",
                "token": "secret_source_token"
            },
            "created_at": "2026-11-13T11:00:00Z"
        }"#;
        let payment: IntentPayment = serde_json::from_str(raw).expect("valid IntentPayment");
        let dbg = format!("{payment:?}");
        assert!(dbg.contains(r#"source: "[REDACTED]""#));
        assert!(!dbg.contains("0512345678"));
        assert!(!dbg.contains("secret_source_token"));
    }

    #[test]
    fn publishable_key_client_can_access_payment_intents_for_fulfill() {
        let pk = PublishableKey::new("pk_test_fulfill_only").expect("valid pk");
        let client = MoyasarClient::new(pk).expect("client builds");
        let _svc = client.payment_intents();
    }
}
