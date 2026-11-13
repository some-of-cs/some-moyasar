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
            self.client.config().base_url.trim_end_matches('/'),
            PAYMENT_INTENTS_PATH,
            id.as_str()
        );
        retrieve_from_url(
            self.client.http(),
            self.client.config().max_network_retries,
            &url,
        )
        .await
    }
}

async fn retrieve_from_url(
    http: &reqwest::Client,
    retries: u32,
    url: &str,
) -> Result<PaymentIntent, MoyasarError> {
    execute_with_retry(retries, || async {
        let resp = http.get(url).send().await?;
        parse_retrieve_response(resp).await
    })
    .await
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
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn tc_3_1_0_02_retrieves_intent_with_client_secret_and_maps_404_over_wire() {
        let server = MockServer::start().await;
        let ok_body = r#"{
            "id": "pi_ret_1",
            "status": "created",
            "amount": 25000,
            "currency": "SAR",
            "callback_url": "https://example.com/cb",
            "client_secret": "cs_retrieved_secret",
            "expires_at": "2026-11-13T12:00:00Z",
            "created_at": "2026-11-13T11:00:00Z"
        }"#;
        Mock::given(method("GET"))
            .and(path("/v1/payment_intents/pi_ret_1"))
            .respond_with(ResponseTemplate::new(200).set_body_string(ok_body))
            .expect(1)
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let ok_url = format!("{}{}/pi_ret_1", server.uri(), PAYMENT_INTENTS_PATH);
        let intent = retrieve_from_url(&http, 2, &ok_url)
            .await
            .expect("retrieved");
        assert_eq!(intent.id.as_str(), "pi_ret_1");
        assert_eq!(intent.client_secret(), Some("cs_retrieved_secret"));

        let not_found_body = r#"{"type":"record_not_found","message":"Payment intent not found"}"#;
        Mock::given(method("GET"))
            .and(path("/v1/payment_intents/pi_missing"))
            .respond_with(ResponseTemplate::new(404).set_body_string(not_found_body))
            .expect(1)
            .mount(&server)
            .await;

        let missing_url = format!("{}{}/pi_missing", server.uri(), PAYMENT_INTENTS_PATH);
        let err = retrieve_from_url(&http, 2, &missing_url)
            .await
            .expect_err("must map 404 to RecordNotFound");
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
