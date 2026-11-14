//! Webhook delivery attempt diagnostics (US-7.2.0 Unit 4).

use serde::Serialize;
use uuid::Uuid;

use super::WebhooksService;
use super::models::WebhookAttempt;
use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::pagination::PaginatedList;
use crate::kernel::transport::execute_with_retry;

/// Query filter and pagination parameters for `GET /v1/webhooks/attempts`.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::webhooks_api::attempts::ListWebhookAttemptsParams;
/// use uuid::Uuid;
///
/// let webhook_id = Uuid::nil();
/// let params = ListWebhookAttemptsParams::new()
///     .page(2)
///     .webhook_id(webhook_id);
/// assert_eq!(params.page, Some(2));
/// assert_eq!(params.webhook_id, Some(webhook_id));
/// assert_eq!(params.event_id, None);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ListWebhookAttemptsParams {
    /// 1-based page number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Filter delivery attempts by registered webhook endpoint ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_id: Option<Uuid>,
    /// Filter delivery attempts by webhook event ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,
}

impl ListWebhookAttemptsParams {
    /// Creates an empty [`ListWebhookAttemptsParams`] with no filters set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the 1-based `page` query parameter.
    #[must_use]
    pub fn page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }

    /// Filters delivery attempts by the target `webhook_id`.
    #[must_use]
    pub fn webhook_id(mut self, webhook_id: Uuid) -> Self {
        self.webhook_id = Some(webhook_id);
        self
    }

    /// Filters delivery attempts by the associated `event_id`.
    #[must_use]
    pub fn event_id(mut self, event_id: Uuid) -> Self {
        self.event_id = Some(event_id);
        self
    }
}

impl<'a, K: SecretAuthKey> WebhooksService<'a, K> {
    /// Lists webhook delivery attempts matching optional [`ListWebhookAttemptsParams`]
    /// via `GET /v1/webhooks/attempts`.
    ///
    /// Restricted at compile time to [`SecretAuthKey`] clients (`SecretKey`).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    /// use some_moyasar::resources::webhooks_api::attempts::ListWebhookAttemptsParams;
    ///
    /// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
    /// let key = SecretKey::new("sk_test_YOUR_KEY")?;
    /// let client = MoyasarClient::new(key)?;
    /// let params = ListWebhookAttemptsParams::new().page(1);
    /// let page = client.webhooks().list_attempts(Some(&params)).await?;
    /// println!("attempts count: {}", page.items.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_attempts(
        &self,
        params: Option<&ListWebhookAttemptsParams>,
    ) -> Result<PaginatedList<WebhookAttempt>, MoyasarError> {
        let base_url = self.client().config().base_url.trim_end_matches('/');
        let url = format!("{base_url}/v1/webhooks/attempts");
        let http = self.client().http();
        let retries = self.client().config().max_network_retries;

        execute_with_retry(retries, || async {
            let mut req = http.get(&url);
            if let Some(filter) = params {
                req = req.query(filter);
            }
            let response = req.send().await.map_err(map_reqwest_error)?;
            parse_attempts_list_response(response).await
        })
        .await
    }

    /// Retrieves a single webhook delivery attempt by its UUID via
    /// `GET /v1/webhooks/attempts/{id}`.
    ///
    /// Restricted at compile time to [`SecretAuthKey`] clients (`SecretKey`).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    /// use uuid::Uuid;
    ///
    /// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
    /// let key = SecretKey::new("sk_test_YOUR_KEY")?;
    /// let client = MoyasarClient::new(key)?;
    /// let attempt_id = Uuid::nil();
    /// let attempt = client.webhooks().retrieve_attempt(attempt_id).await?;
    /// println!("response code: {:?}", attempt.response_code);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn retrieve_attempt(&self, id: Uuid) -> Result<WebhookAttempt, MoyasarError> {
        let base_url = self.client().config().base_url.trim_end_matches('/');
        let url = format!("{base_url}/v1/webhooks/attempts/{id}");
        let http = self.client().http();
        let retries = self.client().config().max_network_retries;

        execute_with_retry(retries, || async {
            let response = http.get(&url).send().await.map_err(map_reqwest_error)?;
            parse_attempt_response(response).await
        })
        .await
    }
}

async fn parse_attempts_list_response(
    response: reqwest::Response,
) -> Result<PaginatedList<WebhookAttempt>, MoyasarError> {
    let status = response.status();
    let body = response.text().await.map_err(map_reqwest_error)?;
    if !status.is_success() {
        return Err(MoyasarError::from_api_response(status.as_u16(), &body));
    }
    serde_json::from_str::<PaginatedList<WebhookAttempt>>(&body).map_err(|err| {
        MoyasarError::ServerError {
            status: status.as_u16(),
            message: format!("failed to decode PaginatedList<WebhookAttempt>: {err}"),
        }
    })
}

async fn parse_attempt_response(
    response: reqwest::Response,
) -> Result<WebhookAttempt, MoyasarError> {
    let status = response.status();
    let body = response.text().await.map_err(map_reqwest_error)?;
    if !status.is_success() {
        return Err(MoyasarError::from_api_response(status.as_u16(), &body));
    }
    serde_json::from_str::<WebhookAttempt>(&body).map_err(|err| MoyasarError::ServerError {
        status: status.as_u16(),
        message: format!("failed to decode WebhookAttempt response: {err}"),
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
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn list_attempts_params_default_omits_all_query_fields() {
        let params = ListWebhookAttemptsParams::new();
        let req = reqwest::Client::new()
            .get("https://api.moyasar.com/v1/webhooks/attempts")
            .query(&params)
            .build()
            .expect("builds request");
        assert_eq!(req.url().query(), None);
    }

    #[test]
    fn list_attempts_params_serializes_non_none_query_fields() {
        let webhook_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
        let event_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440002").unwrap();
        let params = ListWebhookAttemptsParams::new()
            .page(2)
            .webhook_id(webhook_id)
            .event_id(event_id);

        let req = reqwest::Client::new()
            .get("https://api.moyasar.com/v1/webhooks/attempts")
            .query(&params)
            .build()
            .expect("builds request");
        assert_eq!(
            req.url().query(),
            Some(
                "page=2&webhook_id=550e8400-e29b-41d4-a716-446655440001&event_id=550e8400-e29b-41d4-a716-446655440002"
            )
        );
    }

    #[tokio::test]
    async fn tc_7_2_0_04_list_attempts_deserializes_paginated_response() {
        let server = MockServer::start().await;
        let attempt_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440010").unwrap();
        let webhook_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440011").unwrap();
        let event_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440012").unwrap();

        Mock::given(method("GET"))
            .and(path("/v1/webhooks/attempts"))
            .and(query_param("page", "1"))
            .and(query_param("webhook_id", webhook_id.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "attempts": [{
                    "id": attempt_id,
                    "webhook_id": webhook_id,
                    "event_id": event_id,
                    "event_type": "payment_paid",
                    "response_code": 200,
                    "response_headers": { "content-type": "application/json" },
                    "response_body": "ok",
                    "succeeded": true,
                    "message": null,
                    "created_at": "2026-11-14T03:00:00Z"
                }],
                "meta": {
                    "current_page": 1,
                    "next_page": null,
                    "prev_page": null,
                    "total_pages": 1,
                    "total_count": 1
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let params = ListWebhookAttemptsParams::new()
            .page(1)
            .webhook_id(webhook_id);
        let client = MoyasarClient::builder(SecretKey::new("sk_test_attempts").unwrap())
            .base_url(server.uri())
            .max_network_retries(1)
            .build()
            .expect("client builds");

        let page = client
            .webhooks()
            .list_attempts(Some(&params))
            .await
            .expect("lists webhook attempts");

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, attempt_id);
        assert_eq!(page.items[0].webhook_id, Some(webhook_id));
        assert_eq!(page.items[0].event_id, event_id);
        assert_eq!(page.items[0].response_code, Some(200));
        assert_eq!(page.items[0].succeeded, Some(true));
        assert_eq!(page.meta.total_count, 1);
    }

    #[tokio::test]
    async fn list_attempts_maps_401_to_authentication_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/webhooks/attempts"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "type": "authentication_error",
                "message": "Invalid credentials"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = MoyasarClient::builder(SecretKey::new("sk_test_attempts").unwrap())
            .base_url(server.uri())
            .max_network_retries(1)
            .build()
            .expect("client builds");
        let err = client
            .webhooks()
            .list_attempts(None)
            .await
            .expect_err("401 maps to AuthenticationError");

        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::AuthenticationError,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn tc_7_2_0_04_retrieve_attempt_deserializes_and_maps_404() {
        let server = MockServer::start().await;
        let attempt_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440020").unwrap();
        let event_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440021").unwrap();

        Mock::given(method("GET"))
            .and(path(format!("/v1/webhooks/attempts/{attempt_id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": attempt_id,
                "webhook_id": null,
                "event_id": event_id,
                "event_type": "payment_failed",
                "response_code": 502,
                "response_headers": null,
                "response_body": "Bad Gateway",
                "succeeded": false,
                "message": "Upstream returned 502",
                "created_at": "2026-11-14T03:05:00Z"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = MoyasarClient::builder(SecretKey::new("sk_test_attempts").unwrap())
            .base_url(server.uri())
            .max_network_retries(1)
            .build()
            .expect("client builds");
        let attempt = client
            .webhooks()
            .retrieve_attempt(attempt_id)
            .await
            .expect("retrieves webhook attempt");

        assert_eq!(attempt.id, attempt_id);
        assert_eq!(attempt.webhook_id, None);
        assert_eq!(attempt.event_id, event_id);
        assert_eq!(attempt.response_code, Some(502));
        assert_eq!(attempt.response_body.as_deref(), Some("Bad Gateway"));
        assert_eq!(attempt.succeeded, Some(false));

        let missing_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440099").unwrap();
        Mock::given(method("GET"))
            .and(path(format!("/v1/webhooks/attempts/{missing_id}")))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "type": "record_not_found",
                "message": "Webhook attempt not found"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let err = client
            .webhooks()
            .retrieve_attempt(missing_id)
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
