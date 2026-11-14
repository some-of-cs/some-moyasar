//! Webhook endpoint retrieval (`GET /v1/webhooks/{id}`) and deletion (`DELETE /v1/webhooks/{id}`).

use uuid::Uuid;

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::transport::execute_with_retry;
use crate::resources::webhooks_api::WebhooksService;
use crate::resources::webhooks_api::models::WebhookEndpoint;

const WEBHOOKS_PATH: &str = "/v1/webhooks";

impl<'a, K: SecretAuthKey> WebhooksService<'a, K> {
    /// Retrieves a registered webhook endpoint by its UUID (`GET /v1/webhooks/{id}`).
    ///
    /// Requires a [`SecretAuthKey`] (such as [`SecretKey`](crate::kernel::auth::SecretKey));
    /// publishable keys are rejected at compile time.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::Api`] with
    /// [`ApiErrorType::RecordNotFound`](crate::kernel::error::ApiErrorType::RecordNotFound)
    /// when the webhook endpoint does not exist (HTTP 404),
    /// [`ApiErrorType::AuthenticationError`](crate::kernel::error::ApiErrorType::AuthenticationError)
    /// on invalid credentials (HTTP 401), [`MoyasarError::ServerError`] for 5xx responses
    /// after retries are exhausted, or [`MoyasarError::Network`] / [`MoyasarError::Timeout`]
    /// on transport failures.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    /// use uuid::Uuid;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let key = SecretKey::new("sk_test_abc123")?;
    /// let client = MoyasarClient::new(key)?;
    /// let endpoint = client.webhooks().retrieve(Uuid::nil()).await?;
    /// assert!(endpoint.url.starts_with("https://"));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// A [`PublishableKey`](crate::kernel::auth::PublishableKey) cannot call `retrieve()`:
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use uuid::Uuid;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let pk = PublishableKey::new("pk_test_abc123")?;
    /// let client = MoyasarClient::new(pk)?;
    /// let _endpoint = client.webhooks().retrieve(Uuid::nil()).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn retrieve(&self, id: Uuid) -> Result<WebhookEndpoint, MoyasarError> {
        let client = self.client();
        retrieve_webhook(
            client.http(),
            &client.config().base_url,
            client.config().max_network_retries,
            id,
        )
        .await
    }

    /// Deletes a registered webhook endpoint by its UUID (`DELETE /v1/webhooks/{id}`).
    ///
    /// Handles both `200 OK` and `204 No Content` responses as `Ok(())`. Requires a
    /// [`SecretAuthKey`] (such as [`SecretKey`](crate::kernel::auth::SecretKey));
    /// publishable keys are rejected at compile time.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::Api`] for 4xx API errors (such as `404` when the webhook
    /// endpoint does not exist or `401` on invalid credentials),
    /// [`MoyasarError::ServerError`] for 5xx responses after retries are exhausted, or
    /// [`MoyasarError::Network`] / [`MoyasarError::Timeout`] on transport failures.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    /// use uuid::Uuid;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let key = SecretKey::new("sk_test_abc123")?;
    /// let client = MoyasarClient::new(key)?;
    /// client.webhooks().delete(Uuid::nil()).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// A [`PublishableKey`](crate::kernel::auth::PublishableKey) cannot call `delete()`:
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use uuid::Uuid;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let pk = PublishableKey::new("pk_test_abc123")?;
    /// let client = MoyasarClient::new(pk)?;
    /// client.webhooks().delete(Uuid::nil()).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete(&self, id: Uuid) -> Result<(), MoyasarError> {
        let client = self.client();
        delete_webhook(
            client.http(),
            &client.config().base_url,
            client.config().max_network_retries,
            id,
        )
        .await
    }
}

async fn retrieve_webhook(
    http: &reqwest::Client,
    base_url: &str,
    max_network_retries: u32,
    id: Uuid,
) -> Result<WebhookEndpoint, MoyasarError> {
    let url = format!("{}{WEBHOOKS_PATH}/{id}", base_url.trim_end_matches('/'));
    execute_with_retry(max_network_retries, || send_retrieve_request(http, &url)).await
}

async fn send_retrieve_request(
    http: &reqwest::Client,
    url: &str,
) -> Result<WebhookEndpoint, MoyasarError> {
    let response = http.get(url).send().await.map_err(MoyasarError::from)?;
    let status = response.status();
    let body = response.text().await.map_err(MoyasarError::from)?;
    if !status.is_success() {
        return Err(MoyasarError::from_api_response(status.as_u16(), &body));
    }
    serde_json::from_str::<WebhookEndpoint>(&body).map_err(|err| MoyasarError::ServerError {
        status: status.as_u16(),
        message: format!("failed to decode WebhookEndpoint response: {err}"),
    })
}

async fn delete_webhook(
    http: &reqwest::Client,
    base_url: &str,
    max_network_retries: u32,
    id: Uuid,
) -> Result<(), MoyasarError> {
    let url = format!("{}{WEBHOOKS_PATH}/{id}", base_url.trim_end_matches('/'));
    execute_with_retry(max_network_retries, || send_delete_request(http, &url)).await
}

async fn send_delete_request(http: &reqwest::Client, url: &str) -> Result<(), MoyasarError> {
    let response = http.delete(url).send().await.map_err(MoyasarError::from)?;
    let status = response.status();
    if status == reqwest::StatusCode::NO_CONTENT || status.is_success() {
        return Ok(());
    }
    let body = response.text().await.unwrap_or_default();
    Err(MoyasarError::from_api_response(status.as_u16(), &body))
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::client::MoyasarClient;
    use crate::kernel::error::ApiErrorType;
    use crate::kernel::transport::{API_VERSION, MOYASAR_VERSION_HEADER};
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn ac_us_7_2_0_03_retrieve_returns_webhook_endpoint_on_200_ok() {
        let server = MockServer::start().await;
        let webhook_id =
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").expect("valid uuid");
        let expected_path = format!("/v1/webhooks/{webhook_id}");

        Mock::given(method("GET"))
            .and(path(&expected_path))
            .and(header(MOYASAR_VERSION_HEADER, API_VERSION))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": webhook_id.to_string(),
                "http_method": "post",
                "url": "https://merchant.example.com/webhooks/moyasar",
                "shared_secret": "whsec_test_secret_123",
                "events": ["payment_paid", "payment_failed"],
                "created_at": "2026-01-15T10:30:00Z"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_webhook_retrieve").expect("valid secret key");
        let client = MoyasarClient::builder(key)
            .max_network_retries(0)
            .build()
            .expect("client builds");

        let endpoint = retrieve_webhook(
            client.http(),
            &server.uri(),
            client.config().max_network_retries,
            webhook_id,
        )
        .await
        .expect("webhook retrieval succeeds");

        assert_eq!(endpoint.id, webhook_id);
        assert_eq!(endpoint.http_method, "post");
        assert_eq!(
            endpoint.url,
            "https://merchant.example.com/webhooks/moyasar"
        );
        assert!(!format!("{endpoint:?}").contains("whsec_test_secret_123"));
    }

    #[tokio::test]
    async fn ac_us_7_2_0_03_delete_handles_200_ok_and_204_no_content() {
        let server = MockServer::start().await;
        let id_200 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").expect("valid uuid");
        let id_204 = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440002").expect("valid uuid");

        Mock::given(method("DELETE"))
            .and(path(format!("/v1/webhooks/{id_200}")))
            .and(header(MOYASAR_VERSION_HEADER, API_VERSION))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("DELETE"))
            .and(path(format!("/v1/webhooks/{id_204}")))
            .and(header(MOYASAR_VERSION_HEADER, API_VERSION))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_webhook_delete").expect("valid secret key");
        let client = MoyasarClient::builder(key)
            .max_network_retries(0)
            .build()
            .expect("client builds");

        let res_200 = delete_webhook(client.http(), &server.uri(), 0, id_200).await;
        assert!(
            res_200.is_ok(),
            "expected Ok(()) for 200 OK, got {res_200:?}"
        );

        let res_204 = delete_webhook(client.http(), &server.uri(), 0, id_204).await;
        assert!(
            res_204.is_ok(),
            "expected Ok(()) for 204 No Content, got {res_204:?}"
        );
    }

    #[tokio::test]
    async fn ac_us_7_2_0_03_retrieve_after_delete_returns_404_record_not_found() {
        let server = MockServer::start().await;
        let webhook_id =
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440003").expect("valid uuid");
        let target_path = format!("/v1/webhooks/{webhook_id}");

        Mock::given(method("DELETE"))
            .and(path(&target_path))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path(&target_path))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "type": "record_not_found",
                "message": "Webhook not found"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_webhook_404").expect("valid secret key");
        let client = MoyasarClient::builder(key)
            .max_network_retries(2)
            .build()
            .expect("client builds");

        delete_webhook(
            client.http(),
            &server.uri(),
            client.config().max_network_retries,
            webhook_id,
        )
        .await
        .expect("delete succeeds");

        let err = retrieve_webhook(
            client.http(),
            &server.uri(),
            client.config().max_network_retries,
            webhook_id,
        )
        .await
        .expect_err("retrieve after delete must return 404 RecordNotFound");

        assert!(!err.is_retryable());
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::RecordNotFound,
                ref message,
                ..
            } if message == "Webhook not found"
        ));
    }

    #[tokio::test]
    async fn delete_maps_401_without_retrying() {
        let server = MockServer::start().await;
        let webhook_id =
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440004").expect("valid uuid");

        Mock::given(method("DELETE"))
            .and(path(format!("/v1/webhooks/{webhook_id}")))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "type": "authentication_error",
                "message": "Invalid key"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_webhook_unauth").expect("valid secret key");
        let client = MoyasarClient::builder(key)
            .max_network_retries(2)
            .build()
            .expect("client builds");

        let err = delete_webhook(
            client.http(),
            &server.uri(),
            client.config().max_network_retries,
            webhook_id,
        )
        .await
        .expect_err("401 must return error without retrying");

        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::AuthenticationError,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn retrieve_and_delete_retry_transient_503_before_succeeding() {
        let server = MockServer::start().await;
        let webhook_id =
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440005").expect("valid uuid");
        let target_path = format!("/v1/webhooks/{webhook_id}");

        Mock::given(method("GET"))
            .and(path(&target_path))
            .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path(&target_path))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": webhook_id.to_string(),
                "http_method": "post",
                "url": "https://merchant.example.com/webhooks/retry",
                "shared_secret": null,
                "events": null,
                "created_at": "2026-01-15T10:30:00Z"
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("DELETE"))
            .and(path(&target_path))
            .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("DELETE"))
            .and(path(&target_path))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_webhook_retry").expect("valid secret key");
        let client = MoyasarClient::builder(key)
            .max_network_retries(2)
            .build()
            .expect("client builds");

        let endpoint = retrieve_webhook(
            client.http(),
            &server.uri(),
            client.config().max_network_retries,
            webhook_id,
        )
        .await
        .expect("retrieve retries 503 and succeeds");
        assert_eq!(endpoint.id, webhook_id);

        let delete_res = delete_webhook(
            client.http(),
            &server.uri(),
            client.config().max_network_retries,
            webhook_id,
        )
        .await;
        assert!(
            delete_res.is_ok(),
            "delete retries 503 and succeeds: {delete_res:?}"
        );
    }
}
