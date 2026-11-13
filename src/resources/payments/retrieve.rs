//! Payment retrieval endpoint (`GET /v1/payments/:id`) for `PaymentsService` (US-2.1.0 Unit 4).

use core::fmt;

use serde::{Deserialize, Serialize};

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::transport::execute_with_retry;
use crate::resources::payments::models::Payment;
use crate::resources::payments::service::PaymentsService;

/// Validated Moyasar payment identifier (`pay_...`) with non-empty suffix enforcement.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PaymentId(String);

impl PaymentId {
    /// Parses and validates a Moyasar payment identifier.
    ///
    /// Requires the `pay_` prefix followed by a non-empty, URL-safe suffix.
    pub fn new(raw: impl Into<String>) -> Result<Self, MoyasarError> {
        let value = raw.into();
        let valid = value.strip_prefix("pay_").is_some_and(|suffix| {
            !suffix.is_empty()
                && !suffix.chars().any(char::is_whitespace)
                && !suffix.contains('/')
                && !suffix.contains('?')
                && !suffix.contains('#')
        });
        if !valid {
            return Err(MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                message: "invalid payment id: expected non-empty URL-safe suffix after 'pay_'"
                    .to_owned(),
                errors: Vec::new(),
                response_code: None,
            });
        }
        Ok(Self(value))
    }

    /// Returns the underlying payment identifier string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PaymentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for PaymentId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for PaymentId {
    type Error = MoyasarError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for PaymentId {
    type Error = MoyasarError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<'a, K: SecretAuthKey> PaymentsService<'a, K> {
    /// Retrieves a payment by its [`PaymentId`] via `GET /v1/payments/:id`.
    ///
    /// Restricted at compile time to [`SecretAuthKey`] clients (`SecretKey`).
    /// Attempting to call `retrieve` on a `PublishableKey` client fails to compile:
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::payments::retrieve::PaymentId;
    ///
    /// # async fn run() {
    /// let key = PublishableKey::new("pk_test_YOUR_KEY").unwrap();
    /// let client = MoyasarClient::new(key).unwrap();
    /// let id = PaymentId::new("pay_01J9XYZ").unwrap();
    /// let _ = client.payments().retrieve(&id).await;
    /// # }
    /// ```
    pub async fn retrieve(&self, id: &PaymentId) -> Result<Payment, MoyasarError> {
        let url = format!(
            "{}/v1/payments/{}",
            self.client().config().base_url,
            id.as_str()
        );
        let http = self.client().http();
        let retries = self.client().config().max_network_retries;

        execute_with_retry(retries, || async {
            let response = http.get(&url).send().await?;
            parse_retrieve_response(response).await
        })
        .await
    }

    /// Alias for [`PaymentsService::retrieve`].
    pub async fn fetch(&self, id: &PaymentId) -> Result<Payment, MoyasarError> {
        self.retrieve(id).await
    }
}

async fn parse_retrieve_response(response: reqwest::Response) -> Result<Payment, MoyasarError> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(MoyasarError::from_api_response(status.as_u16(), &body));
    }
    serde_json::from_str::<Payment>(&body).map_err(|err| MoyasarError::ServerError {
        status: status.as_u16(),
        message: format!("failed to decode Payment response: {err}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn payment_id_validates_pay_prefix_and_non_empty_suffix() {
        assert!(PaymentId::new("pay_01J9XYZ").is_ok());
        assert!(PaymentId::new("pay_").is_err());
        assert!(PaymentId::new("").is_err());
        assert!(PaymentId::new("inv_01J9XYZ").is_err());
        assert!(PaymentId::new("pay_../other").is_err());
        assert!(PaymentId::new("pay_abc?foo=bar").is_err());
    }

    #[tokio::test]
    async fn tc_2_1_0_03_retrieve_returns_payment_and_maps_404_to_record_not_found() {
        let server = MockServer::start().await;
        let id = PaymentId::new("pay_01J9RETRIEVE").expect("valid id");

        Mock::given(method("GET"))
            .and(path("/v1/payments/pay_01J9RETRIEVE"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "pay_01J9RETRIEVE",
                "status": "paid",
                "amount": 15000,
                "amount_format": "150.00 SAR",
                "fee": 350,
                "fee_format": "3.50 SAR",
                "refunded": 0,
                "refunded_format": "0.00 SAR",
                "captured": 15000,
                "captured_format": "150.00 SAR",
                "currency": "SAR",
                "source": { "type": "creditcard" },
                "created_at": "2026-11-13T10:00:00Z",
                "updated_at": "2026-11-13T10:01:00Z"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let ok_url = format!("{}/v1/payments/{}", server.uri(), id.as_str());
        let payment = execute_with_retry(1, || async {
            let resp = http.get(&ok_url).send().await?;
            parse_retrieve_response(resp).await
        })
        .await
        .expect("retrieves payment");
        assert_eq!(payment.id, "pay_01J9RETRIEVE");

        Mock::given(method("GET"))
            .and(path("/v1/payments/pay_missing"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "type": "record_not_found",
                "message": "Payment not found"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let missing_url = format!("{}/v1/payments/pay_missing", server.uri());
        let err = execute_with_retry(1, || async {
            let resp = http.get(&missing_url).send().await?;
            parse_retrieve_response(resp).await
        })
        .await
        .expect_err("404 maps to RecordNotFound");

        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::RecordNotFound,
                ..
            }
        ));
    }
}
