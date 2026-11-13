//! Payment Intent retrieval endpoint (`retrieve`).

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::transport::execute_with_retry;
use crate::resources::payment_intents::models::{IntentId, PaymentIntent};
use crate::resources::payment_intents::service::PaymentIntentsService;

const PAYMENT_INTENTS_PATH: &str = "/v1/payment_intents";

impl<'a, K: SecretAuthKey> PaymentIntentsService<'a, K> {
    /// Retrieves an existing [`PaymentIntent`] by [`IntentId`] (`GET /v1/payment_intents/:id`).
    ///
    /// Requires a backend [`SecretAuthKey`]. Attempting to call `retrieve` with a
    /// [`PublishableKey`](crate::kernel::auth::PublishableKey) fails to compile (`TC-3.1.0-03`):
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::payment_intents::models::IntentId;
    ///
    /// async fn cannot_retrieve_with_pk() {
    ///     let pk = PublishableKey::new("pk_test_abc123").unwrap();
    ///     let client = MoyasarClient::new(pk).unwrap();
    ///     let id = IntentId::new("pi_test_123").unwrap();
    ///     let _ = client.payment_intents().retrieve(&id).await;
    /// }
    /// ```
    pub async fn retrieve(&self, id: &IntentId) -> Result<PaymentIntent, MoyasarError> {
        let url = format!(
            "{}{}/{}",
            self.client.config().base_url,
            PAYMENT_INTENTS_PATH,
            id.as_str()
        );
        let retries = self.client.config().max_network_retries;
        execute_with_retry(retries, || async {
            let resp = self.client.http().get(&url).send().await?;
            parse_retrieve_response(resp).await
        })
        .await
    }
}

async fn parse_retrieve_response(resp: reqwest::Response) -> Result<PaymentIntent, MoyasarError> {
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
    use crate::kernel::error::ApiErrorType;

    #[test]
    fn tc_3_1_0_02_maps_404_to_record_not_found_and_is_not_retryable() {
        let body = r#"{"type":"record_not_found","message":"Payment intent not found"}"#;
        let err = MoyasarError::from_api_response(404, body);
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::RecordNotFound,
                ..
            }
        ));
        assert!(!err.is_retryable());
    }

    #[test]
    fn maps_403_forbidden_to_non_retryable_api_error() {
        let body = r#"{"type":"authentication_error","message":"Publishable key not allowed"}"#;
        let err = MoyasarError::from_api_response(403, body);
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::AuthenticationError,
                ..
            }
        ));
        assert!(!err.is_retryable());
    }

    #[test]
    fn validates_non_empty_intent_id_before_retrieve() {
        assert!(IntentId::new("").is_err());
        assert!(IntentId::new("pi_valid_999").is_ok());
    }
}
