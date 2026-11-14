//! Available webhook events listing and `WebhookEventType` re-export (US-7.2.0 Unit 3).

use serde::Deserialize;

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::transport::execute_with_retry;
use crate::resources::webhooks_api::WebhooksService;

pub use crate::webhook_engine::event_types::WebhookEventType;

#[derive(Deserialize)]
#[serde(untagged)]
enum AvailableEventsEnvelope {
    Array(Vec<WebhookEventType>),
    Wrapped {
        #[serde(alias = "events")]
        available_events: Vec<WebhookEventType>,
    },
}

impl AvailableEventsEnvelope {
    fn into_vec(self) -> Vec<WebhookEventType> {
        match self {
            Self::Array(events) => events,
            Self::Wrapped { available_events } => available_events,
        }
    }
}

impl<'a, K: SecretAuthKey> WebhooksService<'a, K> {
    /// Lists all canonical webhook event types supported by Moyasar (`GET /v1/webhooks/available_events`).
    ///
    /// Returns a vector of [`WebhookEventType`] variants, automatically normalizing
    /// Moyasar's documented `"payment_faild"` alias to [`WebhookEventType::PaymentFailed`]
    /// and preserving unrecognized future events as [`WebhookEventType::Unknown`].
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::Api`] on 4xx API errors, [`MoyasarError::ServerError`]
    /// after exhausting retries on 5xx responses, or [`MoyasarError::Network`] /
    /// [`MoyasarError::Timeout`] on transport failures.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let key = SecretKey::new("sk_test_example")?;
    /// let client = MoyasarClient::new(key)?;
    /// let events = client.webhooks().available_events().await?;
    /// assert!(!events.is_empty());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn available_events(&self) -> Result<Vec<WebhookEventType>, MoyasarError> {
        fetch_available_events(
            self.client().http(),
            &self.client().config().base_url,
            self.client().config().max_network_retries,
        )
        .await
    }
}

async fn fetch_available_events(
    http: &reqwest::Client,
    base_url: &str,
    max_network_retries: u32,
) -> Result<Vec<WebhookEventType>, MoyasarError> {
    let url = format!(
        "{}/v1/webhooks/available_events",
        base_url.trim_end_matches('/')
    );

    execute_with_retry(max_network_retries, || async {
        let response = http.get(&url).send().await.map_err(map_reqwest_error)?;
        parse_available_events_response(response).await
    })
    .await
}

async fn parse_available_events_response(
    response: reqwest::Response,
) -> Result<Vec<WebhookEventType>, MoyasarError> {
    let status = response.status();
    let body = response.text().await.map_err(map_reqwest_error)?;
    if !status.is_success() {
        return Err(MoyasarError::from_api_response(status.as_u16(), &body));
    }
    serde_json::from_str::<AvailableEventsEnvelope>(&body)
        .map(AvailableEventsEnvelope::into_vec)
        .map_err(|err| MoyasarError::ServerError {
            status: status.as_u16(),
            message: format!("failed to decode available webhook events response: {err}"),
        })
}

fn map_reqwest_error(err: reqwest::Error) -> MoyasarError {
    if err.is_timeout() {
        MoyasarError::Timeout
    } else {
        MoyasarError::Network(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::client::MoyasarClient;
    use crate::kernel::error::ApiErrorType;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn tc_7_2_0_04_wiremock_available_events_deserializes_all_11_variants_and_unknown() {
        let server = MockServer::start().await;

        let payload = json!([
            "payment_paid",
            "payment_failed",
            "payment_faild",
            "payment_refunded",
            "payment_voided",
            "payment_authorized",
            "payment_captured",
            "payment_verified",
            "payment_abandoned",
            "payment_expired",
            "card_auth_authenticated",
            "card_auth_failed",
            "settlement_completed_v2"
        ]);

        Mock::given(method("GET"))
            .and(path("/v1/webhooks/available_events"))
            .respond_with(ResponseTemplate::new(200).set_body_json(payload))
            .expect(1)
            .mount(&server)
            .await;

        let client = MoyasarClient::builder(SecretKey::new("sk_test_events").unwrap())
            .base_url(server.uri())
            .max_network_retries(1)
            .build()
            .expect("client builds");
        let events = client
            .webhooks()
            .available_events()
            .await
            .expect("available_events deserializes all variants");

        assert_eq!(
            events,
            vec![
                WebhookEventType::PaymentPaid,
                WebhookEventType::PaymentFailed,
                WebhookEventType::PaymentFailed,
                WebhookEventType::PaymentRefunded,
                WebhookEventType::PaymentVoided,
                WebhookEventType::PaymentAuthorized,
                WebhookEventType::PaymentCaptured,
                WebhookEventType::PaymentVerified,
                WebhookEventType::PaymentAbandoned,
                WebhookEventType::PaymentExpired,
                WebhookEventType::CardAuthAuthenticated,
                WebhookEventType::CardAuthFailed,
                WebhookEventType::Unknown("settlement_completed_v2".to_owned()),
            ]
        );
    }

    #[tokio::test]
    async fn wiremock_available_events_accepts_wrapped_object_and_maps_api_error() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/webhooks/available_events"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "available_events": ["payment_paid", "payment_faild"]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = MoyasarClient::builder(SecretKey::new("sk_test_events").unwrap())
            .base_url(server.uri())
            .max_network_retries(1)
            .build()
            .expect("client builds");
        let wrapped = client
            .webhooks()
            .available_events()
            .await
            .expect("wrapped available_events object decodes");
        assert_eq!(
            wrapped,
            vec![
                WebhookEventType::PaymentPaid,
                WebhookEventType::PaymentFailed
            ]
        );

        let err_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/webhooks/available_events"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "type": "authentication_error",
                "message": "Invalid API key"
            })))
            .expect(1)
            .mount(&err_server)
            .await;

        let err_client = MoyasarClient::builder(SecretKey::new("sk_test_events").unwrap())
            .base_url(err_server.uri())
            .max_network_retries(2)
            .build()
            .expect("client builds");
        let err = err_client
            .webhooks()
            .available_events()
            .await
            .expect_err("401 must fail without retrying");

        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::AuthenticationError,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn wiremock_available_events_retries_503_and_succeeds() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/webhooks/available_events"))
            .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/webhooks/available_events"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!(["payment_paid"])))
            .expect(1)
            .mount(&server)
            .await;

        let client = MoyasarClient::builder(SecretKey::new("sk_test_events").unwrap())
            .base_url(server.uri())
            .max_network_retries(2)
            .build()
            .expect("client builds");
        let events = client
            .webhooks()
            .available_events()
            .await
            .expect("recovers after transient 503");

        assert_eq!(events, vec![WebhookEventType::PaymentPaid]);
    }
}
