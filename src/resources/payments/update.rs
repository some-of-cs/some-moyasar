//! Payment update endpoint (`PUT /v1/payments/:id`) (US-2.2.0 Unit 1).
//!
//! Supports mutating a payment's `description` and `metadata`, including
//! unsetting individual metadata keys by setting their value to an empty string (`""`).

use serde::Serialize;

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::metadata::Metadata;
use crate::kernel::transport::execute_with_retry;
use crate::resources::payments::models::Payment;
use crate::resources::payments::service::PaymentsService;

/// Validates that `id` is a non-empty, URL-safe single path segment.
///
/// Rejects empty strings, whitespace, `/`, `?`, `#`, and `..` traversal sequences
/// before interpolating `id` into Moyasar API request URLs.
pub(crate) fn validate_payment_id(id: &str) -> Result<(), MoyasarError> {
    let invalid = id.is_empty()
        || id.chars().any(char::is_whitespace)
        || id.contains('/')
        || id.contains('?')
        || id.contains('#')
        || id.contains("..");
    if invalid {
        return Err(MoyasarError::Api {
            error_type: ApiErrorType::InvalidRequestError,
            message: "payment id must be a non-empty URL-safe path segment".to_owned(),
            errors: Vec::new(),
            response_code: None,
        });
    }
    Ok(())
}

/// Parameters for updating an existing payment via `PUT /v1/payments/:id`.
///
/// Fields left as `None` are omitted from the JSON payload. To unset an
/// existing metadata key on the server, set its value to `""` via
/// [`UpdatePaymentParams::unset_metadata`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdatePaymentParams {
    /// Updated human-readable description for the payment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Updated metadata entries; empty-string values (`""`) unset keys server-side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

impl UpdatePaymentParams {
    /// Creates an empty [`UpdatePaymentParams`] builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the updated payment description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Replaces the entire metadata map on the update payload.
    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Inserts or updates a single metadata key/value pair after limit validation.
    pub fn metadata_entry(mut self, key: &str, value: &str) -> Result<Self, MoyasarError> {
        let mut map = self.metadata.take().unwrap_or_default();
        map.insert(key, value)?;
        self.metadata = Some(map);
        Ok(self)
    }

    /// Marks `key` to be unset on the server by serializing `"key": ""`.
    pub fn unset_metadata(mut self, key: &str) -> Result<Self, MoyasarError> {
        let mut map = self.metadata.take().unwrap_or_default();
        map.unset(key)?;
        self.metadata = Some(map);
        Ok(self)
    }
}

impl<K: SecretAuthKey> PaymentsService<'_, K> {
    /// Updates an existing payment's `description` and/or `metadata` (`PUT /v1/payments/:id`).
    ///
    /// Requires a [`SecretAuthKey`]; [`PublishableKey`](crate::kernel::auth::PublishableKey)
    /// is rejected at compile time.
    ///
    /// # Examples
    ///
    /// Publishable keys cannot invoke `update`:
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::payments::update::UpdatePaymentParams;
    ///
    /// # async fn example() {
    /// let pk = PublishableKey::new("pk_test_abc123").unwrap();
    /// let client = MoyasarClient::new(pk).unwrap();
    /// let params = UpdatePaymentParams::new().description("Order #42");
    /// let _ = client.payments().update("pay_123", &params).await;
    /// # }
    /// ```
    pub async fn update(
        &self,
        id: &str,
        params: &UpdatePaymentParams,
    ) -> Result<Payment, MoyasarError> {
        validate_payment_id(id)?;

        let url = format!(
            "{}/v1/payments/{id}",
            self.client().config().base_url.trim_end_matches('/')
        );
        let http = self.client().http();
        let retries = self.client().config().max_network_retries;

        execute_with_retry(retries, || async {
            let response = http.put(&url).json(params).send().await?;
            parse_payment_response(response).await
        })
        .await
    }
}

async fn parse_payment_response(response: reqwest::Response) -> Result<Payment, MoyasarError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response.json::<Payment>().await?);
    }
    let body = response.text().await.unwrap_or_default();
    Err(MoyasarError::from_api_response(status.as_u16(), &body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::client::MoyasarClient;
    use crate::resources::payments::models::PaymentStatus;
    use serde_json::json;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn tc_2_2_0_01_serializes_description_and_omits_none_fields() {
        let params = UpdatePaymentParams::new().description("Order #1024");
        let json = serde_json::to_value(&params).expect("serializes params");
        assert_eq!(json["description"], "Order #1024");
        assert!(json.get("metadata").is_none());
    }

    #[test]
    fn tc_2_2_0_02_unset_metadata_serializes_empty_string_verbatim() {
        let params = UpdatePaymentParams::new()
            .unset_metadata("order_id")
            .expect("unset metadata key");
        let json = serde_json::to_value(&params).expect("serializes params");
        assert_eq!(json["metadata"]["order_id"], "");
    }

    #[test]
    fn validate_payment_id_rejects_unsafe_segments() {
        assert!(validate_payment_id("pay_01J9XYZ").is_ok());
        for bad in [
            "",
            "   ",
            "pay_123/void",
            "pay_123?foo=bar",
            "pay_123#frag",
            "../other",
            "pay 123",
        ] {
            let err = validate_payment_id(bad).expect_err("must reject unsafe id");
            assert!(matches!(
                err,
                MoyasarError::Api {
                    error_type: ApiErrorType::InvalidRequestError,
                    ..
                }
            ));
        }
    }

    #[tokio::test]
    async fn update_rejects_invalid_id_before_network() {
        let key = SecretKey::new("sk_test_update_validation").expect("valid key");
        let client = MoyasarClient::new(key).expect("client builds");
        let params = UpdatePaymentParams::new().description("Order #42");
        let err = client
            .payments()
            .update("pay_123/refund", &params)
            .await
            .expect_err("must reject path traversal");
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn tc_2_2_0_01_and_02_wiremock_put_updates_description_and_unsets_metadata() {
        let server = MockServer::start().await;
        let params = UpdatePaymentParams::new()
            .description("Order #1024")
            .unset_metadata("order_id")
            .expect("unset metadata");

        let response_body = json!({
            "id": "pay_01J9UPD",
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
            "description": "Order #1024",
            "source": { "type": "creditcard" },
            "metadata": {},
            "created_at": "2026-11-13T10:00:00Z",
            "updated_at": "2026-11-13T10:02:00Z"
        });

        Mock::given(method("PUT"))
            .and(path("/v1/payments/pay_01J9UPD"))
            .and(body_json(json!({
                "description": "Order #1024",
                "metadata": { "order_id": "" }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
            .expect(1)
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let url = format!("{}/v1/payments/pay_01J9UPD", server.uri());
        let payment = execute_with_retry(1, || async {
            let resp = http.put(&url).json(&params).send().await?;
            parse_payment_response(resp).await
        })
        .await
        .expect("updates payment");

        assert_eq!(payment.id, "pay_01J9UPD");
        assert_eq!(payment.status, PaymentStatus::Paid);
        assert_eq!(payment.description.as_deref(), Some("Order #1024"));
        assert!(payment.metadata.get("order_id").is_none());
    }
}
