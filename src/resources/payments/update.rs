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
        if id.trim().is_empty() {
            return Err(MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                message: "payment id must not be empty".to_owned(),
                errors: Vec::new(),
                response_code: None,
            });
        }

        let url = format!("{}/v1/payments/{id}", self.client().config().base_url);
        let http = self.client().http();
        let retries = self.client().config().max_network_retries;

        execute_with_retry(retries, || async {
            let response = http.put(&url).json(params).send().await?;
            let status = response.status();
            if status.is_success() {
                return Ok(response.json::<Payment>().await?);
            }
            let body = response.text().await.unwrap_or_default();
            Err(MoyasarError::from_api_response(status.as_u16(), &body))
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
