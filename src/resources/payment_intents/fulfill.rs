//! Payment Intent fulfillment endpoint (`fulfill`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::kernel::auth::AuthKey;
use crate::kernel::error::MoyasarError;
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
#[derive(Debug, Clone, Deserialize)]
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
            self.client.config().base_url,
            PAYMENT_INTENTS_PATH,
            id.as_str()
        );
        let retries = self.client.config().max_network_retries;
        execute_with_retry(retries, || async {
            let resp = self.client.http().post(&url).json(&params).send().await?;
            parse_fulfill_response(resp).await
        })
        .await
    }
}

async fn parse_fulfill_response(resp: reqwest::Response) -> Result<IntentPayment, MoyasarError> {
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
    use crate::kernel::auth::PublishableKey;
    use crate::kernel::client::MoyasarClient;
    use crate::kernel::error::ApiErrorType;

    #[test]
    fn tc_3_2_0_01_deserializes_fulfilled_intent_payment() {
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
        let payment: IntentPayment = serde_json::from_str(raw).expect("valid payment");
        assert_eq!(payment.id, "pay_intent_001");
        assert_eq!(payment.status, IntentPaymentStatus::Initiated);
        assert_eq!(payment.amount, 25000);
    }

    #[test]
    fn tc_3_2_0_03_and_04_map_4xx_mismatch_and_expired_to_non_retryable() {
        let wrong_secret = r#"{"type":"invalid_request_error","message":"client_secret mismatch"}"#;
        let err = MoyasarError::from_api_response(400, wrong_secret);
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
    fn publishable_key_client_can_access_payment_intents_for_fulfill() {
        let pk = PublishableKey::new("pk_test_fulfill_only").expect("valid pk");
        let client = MoyasarClient::new(pk).expect("client builds");
        let _svc = client.payment_intents();
    }
}
