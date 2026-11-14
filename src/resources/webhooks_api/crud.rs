//! Webhook endpoint creation and listing (`CreateWebhookParams`, `validate_https_url`, `WebhooksService::create`, `WebhooksService::list`).

use http::header::CONTENT_TYPE;
use serde::Serialize;
use serde::de::DeserializeOwned;
use zeroize::Zeroize;

use crate::kernel::auth::{AuthKey, SecretAuthKey};
use crate::kernel::client::MoyasarClient;
use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::pagination::PaginatedList;
use crate::kernel::transport::execute_with_retry;
use crate::resources::webhooks_api::WebhooksService;
use crate::resources::webhooks_api::models::{WebhookEndpoint, WebhookSecret};
use crate::webhook_engine::event_types::WebhookEventType;

const WEBHOOKS_PATH: &str = "/v1/webhooks";

/// Validates that a webhook callback URL uses the `https://` scheme with a non-empty host.
///
/// Rejects plaintext `http://` URLs, empty strings, bare `"https://"`, whitespace,
/// and any other non-HTTPS schemes before any network call is attempted.
///
/// # Errors
///
/// Returns [`MoyasarError::InvalidWebhookUrl`] if `url` is not a valid `https://` URL.
///
/// # Examples
///
/// ```
/// use some_moyasar::kernel::error::MoyasarError;
/// use some_moyasar::resources::webhooks_api::crud::validate_https_url;
///
/// assert!(validate_https_url("https://merchant.example/webhooks").is_ok());
/// assert!(matches!(
///     validate_https_url("http://merchant.example/webhooks"),
///     Err(MoyasarError::InvalidWebhookUrl)
/// ));
/// assert!(matches!(
///     validate_https_url("https://"),
///     Err(MoyasarError::InvalidWebhookUrl)
/// ));
/// ```
pub fn validate_https_url(url: &str) -> Result<(), MoyasarError> {
    let Some(rest) = url.strip_prefix("https://") else {
        return Err(MoyasarError::InvalidWebhookUrl);
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let valid_host = !host.is_empty()
        && (host.contains('.') || host == "localhost" || host.starts_with("localhost:"));
    if !valid_host || url.chars().any(char::is_whitespace) {
        return Err(MoyasarError::InvalidWebhookUrl);
    }
    Ok(())
}

/// Request payload for registering a Moyasar webhook endpoint (`POST /v1/webhooks`).
///
/// Enforces HTTPS-only callback URLs at construction time via [`validate_https_url`],
/// omits `events` from serialized JSON when `None` (subscribing as a global listener),
/// and delegates `Debug` redaction of `shared_secret` to [`WebhookSecret`].
///
/// # Examples
///
/// ```
/// use some_moyasar::kernel::error::MoyasarError;
/// use some_moyasar::resources::webhooks_api::crud::CreateWebhookParams;
/// use some_moyasar::resources::webhooks_api::models::WebhookSecret;
/// use some_moyasar::webhook_engine::event_types::WebhookEventType;
///
/// let secret = WebhookSecret::new("whsec_test_123");
/// let params = CreateWebhookParams::new("https://merchant.example/webhooks", secret.clone())
///     .unwrap()
///     .events(vec![
///         WebhookEventType::PaymentPaid,
///         WebhookEventType::PaymentFailed,
///     ]);
///
/// assert_eq!(params.http_method, "post");
/// assert!(params.events.is_some());
/// assert!(!format!("{params:?}").contains("whsec_test_123"));
///
/// let invalid = CreateWebhookParams::new("http://merchant.example/webhooks", secret);
/// assert!(matches!(invalid, Err(MoyasarError::InvalidWebhookUrl)));
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct CreateWebhookParams {
    /// HTTP method used by Moyasar when delivering webhook payloads (defaults to `"post"`).
    pub http_method: String,
    /// HTTPS destination URL that receives webhook events.
    pub url: String,
    /// Caller-supplied HMAC shared secret used to sign webhook payloads.
    pub shared_secret: WebhookSecret,
    /// Optional list of subscribed event types; `None` subscribes to all events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<WebhookEventType>>,
}

impl CreateWebhookParams {
    /// Creates a new [`CreateWebhookParams`] with `http_method` defaulted to `"post"`
    /// and `events` set to `None` (global listener).
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::InvalidWebhookUrl`] if `url` is not a valid `https://` URL.
    pub fn new(url: impl Into<String>, shared_secret: WebhookSecret) -> Result<Self, MoyasarError> {
        let url = url.into();
        validate_https_url(&url)?;
        Ok(Self {
            http_method: "post".to_owned(),
            url,
            shared_secret,
            events: None,
        })
    }

    /// Overrides the HTTP delivery method (default `"post"`).
    #[must_use]
    pub fn http_method(mut self, http_method: impl Into<String>) -> Self {
        self.http_method = http_method.into();
        self
    }

    /// Restricts the webhook endpoint to the specified [`WebhookEventType`] list.
    #[must_use]
    pub fn events(mut self, events: Vec<WebhookEventType>) -> Self {
        self.events = Some(events);
        self
    }
}

impl<'a, K: SecretAuthKey> WebhooksService<'a, K> {
    /// Registers a new webhook endpoint (`POST /v1/webhooks`).
    ///
    /// Requires a [`SecretAuthKey`] (e.g., [`SecretKey`](crate::kernel::auth::SecretKey)).
    /// Validates that `params.url` uses `https://` before any network call is attempted.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::InvalidWebhookUrl`] if `params.url` is not HTTPS,
    /// or [`MoyasarError`] on network failure, timeout, or non-2xx API response.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    /// use some_moyasar::resources::webhooks_api::crud::CreateWebhookParams;
    /// use some_moyasar::resources::webhooks_api::models::WebhookSecret;
    ///
    /// # async fn example() -> Result<(), some_moyasar::prelude::MoyasarError> {
    /// let client = MoyasarClient::new(SecretKey::new("sk_test_example")?)?;
    /// let secret = WebhookSecret::new("whsec_high_entropy_secret");
    /// let params = CreateWebhookParams::new("https://merchant.example/webhooks", secret)?;
    /// let endpoint = client.webhooks().create(&params).await?;
    /// println!("Registered webhook: {}", endpoint.id);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// A [`PublishableKey`](crate::kernel::auth::PublishableKey) cannot call
    /// `create` (`SecretAuthKey` is required):
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::webhooks_api::crud::CreateWebhookParams;
    /// use some_moyasar::resources::webhooks_api::models::WebhookSecret;
    ///
    /// # async fn example() {
    /// let pk = PublishableKey::new("pk_test_example").unwrap();
    /// let client = MoyasarClient::new(pk).unwrap();
    /// let secret = WebhookSecret::new("whsec_high_entropy_secret");
    /// let params = CreateWebhookParams::new("https://merchant.example/webhooks", secret).unwrap();
    /// let _ = client.webhooks().create(&params).await;
    /// # }
    /// ```
    pub async fn create(
        &self,
        params: &CreateWebhookParams,
    ) -> Result<WebhookEndpoint, MoyasarError> {
        let client = self.client();
        post_webhook(client, &client.config().base_url, params).await
    }

    /// Lists registered webhook endpoints with optional pagination (`GET /v1/webhooks`).
    ///
    /// Requires a [`SecretAuthKey`] (e.g., [`SecretKey`](crate::kernel::auth::SecretKey)).
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError`] on network failure, timeout, or non-2xx API response.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    ///
    /// # async fn example() -> Result<(), some_moyasar::prelude::MoyasarError> {
    /// let client = MoyasarClient::new(SecretKey::new("sk_test_example")?)?;
    /// let page = client.webhooks().list(Some(1)).await?;
    /// println!("Configured webhooks: {}", page.meta.total_count);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// A [`PublishableKey`](crate::kernel::auth::PublishableKey) cannot call
    /// `list` (`SecretAuthKey` is required):
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    ///
    /// # async fn example() {
    /// let pk = PublishableKey::new("pk_test_example").unwrap();
    /// let client = MoyasarClient::new(pk).unwrap();
    /// let _ = client.webhooks().list(None).await;
    /// # }
    /// ```
    pub async fn list(
        &self,
        page: Option<u32>,
    ) -> Result<PaginatedList<WebhookEndpoint>, MoyasarError> {
        let client = self.client();
        get_webhooks(client, &client.config().base_url, page).await
    }
}

async fn post_webhook<K: AuthKey>(
    client: &MoyasarClient<K>,
    base_url: &str,
    params: &CreateWebhookParams,
) -> Result<WebhookEndpoint, MoyasarError> {
    validate_https_url(&params.url)?;
    let url = format!("{}{WEBHOOKS_PATH}", base_url.trim_end_matches('/'));
    let max_retries = client.config().max_network_retries;
    let (status, body) = execute_with_retry(max_retries, || async {
        dispatch_post(client.http(), &url, params).await
    })
    .await?;
    decode_response(status, &body)
}

async fn get_webhooks<K: AuthKey>(
    client: &MoyasarClient<K>,
    base_url: &str,
    page: Option<u32>,
) -> Result<PaginatedList<WebhookEndpoint>, MoyasarError> {
    let url = format!("{}{WEBHOOKS_PATH}", base_url.trim_end_matches('/'));
    let query: Vec<(&str, String)> = page
        .into_iter()
        .map(|value| ("page", value.to_string()))
        .collect();
    let max_retries = client.config().max_network_retries;
    let (status, body) = execute_with_retry(max_retries, || async {
        dispatch_get(client.http(), &url, &query).await
    })
    .await?;
    decode_response(status, &body)
}

async fn dispatch_post(
    http: &reqwest::Client,
    url: &str,
    params: &CreateWebhookParams,
) -> Result<(u16, String), MoyasarError> {
    let mut payload = serde_json::to_vec(params).map_err(|err| MoyasarError::Api {
        error_type: ApiErrorType::InvalidRequestError,
        message: format!("failed to serialize webhook request: {err}"),
        errors: Vec::new(),
        response_code: None,
    })?;
    let send_res = http
        .post(url)
        .header(CONTENT_TYPE, "application/json")
        .body(payload.clone())
        .send()
        .await;
    payload.zeroize();
    let response = send_res.map_err(MoyasarError::from)?;
    read_response_text(response).await
}

async fn dispatch_get(
    http: &reqwest::Client,
    url: &str,
    query: &[(&str, String)],
) -> Result<(u16, String), MoyasarError> {
    let response = http
        .get(url)
        .query(query)
        .send()
        .await
        .map_err(MoyasarError::from)?;
    read_response_text(response).await
}

async fn read_response_text(response: reqwest::Response) -> Result<(u16, String), MoyasarError> {
    let status = response.status().as_u16();
    let body = response.text().await.map_err(MoyasarError::from)?;
    if status >= 500 {
        return Err(MoyasarError::from_api_response(status, &body));
    }
    Ok((status, body))
}

fn decode_response<T: DeserializeOwned>(status: u16, body: &str) -> Result<T, MoyasarError> {
    if !(200..300).contains(&status) {
        return Err(MoyasarError::from_api_response(status, body));
    }
    serde_json::from_str::<T>(body).map_err(|err| MoyasarError::Api {
        error_type: ApiErrorType::Unknown,
        message: format!("failed to decode webhook response: {err}"),
        errors: Vec::new(),
        response_code: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::transport::{API_VERSION, MOYASAR_VERSION_HEADER};
    use serde_json::json;
    use uuid::Uuid;
    use wiremock::matchers::{body_partial_json, header, header_exists, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_client() -> MoyasarClient<SecretKey> {
        let key = SecretKey::new("sk_test_webhooks_unit2").expect("valid secret key");
        MoyasarClient::new(key).expect("client builds")
    }

    fn sample_endpoint_json(id: Uuid, events: serde_json::Value) -> serde_json::Value {
        json!({
            "id": id,
            "http_method": "post",
            "url": "https://merchant.example/webhooks/moyasar",
            "shared_secret": "whsec_sample_secret",
            "events": events,
            "created_at": "2026-11-14T00:00:00Z"
        })
    }

    #[test]
    fn tc_7_2_0_02_omits_events_when_none_and_redacts_secret_debug() {
        let secret = WebhookSecret::new("whsec_top_secret_value");
        let global = CreateWebhookParams::new("https://merchant.example/webhooks", secret.clone())
            .expect("valid https url");

        let dbg = format!("{global:?}");
        assert!(dbg.contains("[REDACTED]"));
        assert!(!dbg.contains("whsec_top_secret_value"));

        let global_json = serde_json::to_value(&global).expect("serialize global");
        assert_eq!(global_json["http_method"], "post");
        assert_eq!(global_json["url"], "https://merchant.example/webhooks");
        assert_eq!(global_json["shared_secret"], "whsec_top_secret_value");
        assert!(global_json.get("events").is_none());

        let filtered = global.events(vec![
            WebhookEventType::PaymentPaid,
            WebhookEventType::PaymentFailed,
        ]);
        let filtered_json = serde_json::to_value(&filtered).expect("serialize filtered");
        assert_eq!(filtered_json["events"].as_array().map(Vec::len), Some(2));
    }

    #[tokio::test]
    async fn tc_7_2_0_05_rejects_non_https_urls_with_zero_network_calls() {
        for bad_url in [
            "http://merchant.example/webhooks",
            "ftp://merchant.example/webhooks",
            "https://",
            "https:///webhooks",
            "https://merchant.example/webhooks with spaces",
            "",
        ] {
            assert!(matches!(
                validate_https_url(bad_url),
                Err(MoyasarError::InvalidWebhookUrl)
            ));
            let err = CreateWebhookParams::new(bad_url, WebhookSecret::new("whsec_123"))
                .expect_err("non-https or malformed URL must fail");
            assert!(matches!(err, MoyasarError::InvalidWebhookUrl));
        }

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/webhooks"))
            .respond_with(ResponseTemplate::new(201))
            .expect(0)
            .mount(&server)
            .await;

        let mut params = CreateWebhookParams::new(
            "https://merchant.example/webhooks",
            WebhookSecret::new("whsec_123"),
        )
        .expect("valid initial url");
        params.url = "http://merchant.example/webhooks".to_owned();

        let client = test_client();
        let err = post_webhook(&client, &server.uri(), &params)
            .await
            .expect_err("mutated http url must fail before transport");
        assert!(matches!(err, MoyasarError::InvalidWebhookUrl));
    }

    #[tokio::test]
    async fn tc_7_2_0_01_wiremock_create_webhook_posts_expected_payload() {
        let server = MockServer::start().await;
        let endpoint_id = Uuid::new_v4();

        Mock::given(method("POST"))
            .and(path("/v1/webhooks"))
            .and(header_exists("authorization"))
            .and(header(MOYASAR_VERSION_HEADER, API_VERSION))
            .and(body_partial_json(json!({
                "http_method": "post",
                "url": "https://merchant.example/webhooks/moyasar",
                "shared_secret": "whsec_sample_secret",
                "events": ["payment_paid", "payment_failed"]
            })))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(sample_endpoint_json(
                    endpoint_id,
                    json!(["payment_paid", "payment_failed"]),
                )),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = test_client();
        let params = CreateWebhookParams::new(
            "https://merchant.example/webhooks/moyasar",
            WebhookSecret::new("whsec_sample_secret"),
        )
        .expect("valid url")
        .events(vec![
            WebhookEventType::PaymentPaid,
            WebhookEventType::PaymentFailed,
        ]);

        let endpoint = post_webhook(&client, &server.uri(), &params)
            .await
            .expect("create succeeds");
        assert_eq!(endpoint.id, endpoint_id);
        assert_eq!(endpoint.http_method, "post");
        assert_eq!(endpoint.url, "https://merchant.example/webhooks/moyasar");
        assert_eq!(
            endpoint.events,
            Some(vec![
                WebhookEventType::PaymentPaid,
                WebhookEventType::PaymentFailed,
            ])
        );
    }

    #[tokio::test]
    async fn tc_7_2_0_03_wiremock_list_webhooks_with_and_without_page() {
        let server = MockServer::start().await;
        let endpoint_id = Uuid::new_v4();

        Mock::given(method("GET"))
            .and(path("/v1/webhooks"))
            .and(header_exists("authorization"))
            .and(header(MOYASAR_VERSION_HEADER, API_VERSION))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "webhooks": [sample_endpoint_json(endpoint_id, serde_json::Value::Null)],
                "meta": {
                    "current_page": 2,
                    "next_page": null,
                    "prev_page": 1,
                    "total_pages": 2,
                    "total_count": 3
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = test_client();
        let page = get_webhooks(&client, &server.uri(), Some(2))
            .await
            .expect("list page 2 succeeds");
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, endpoint_id);
        assert_eq!(page.meta.current_page, 2);
        assert_eq!(page.meta.prev_page, Some(1));
        assert_eq!(page.meta.total_count, 3);
    }

    #[tokio::test]
    async fn wiremock_retries_503_and_stops_on_422() {
        let server = MockServer::start().await;
        let endpoint_id = Uuid::new_v4();

        Mock::given(method("POST"))
            .and(path("/v1/webhooks"))
            .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/webhooks"))
            .respond_with(
                ResponseTemplate::new(201)
                    .set_body_json(sample_endpoint_json(endpoint_id, serde_json::Value::Null)),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = test_client();
        let params = CreateWebhookParams::new(
            "https://merchant.example/webhooks/moyasar",
            WebhookSecret::new("whsec_retry"),
        )
        .expect("valid url");
        let recovered = post_webhook(&client, &server.uri(), &params)
            .await
            .expect("recovers after 503");
        assert_eq!(recovered.id, endpoint_id);

        let err_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/webhooks"))
            .respond_with(ResponseTemplate::new(422).set_body_json(json!({
                "type": "invalid_request_error",
                "message": "Invalid webhook URL"
            })))
            .expect(1)
            .mount(&err_server)
            .await;

        let err = post_webhook(&client, &err_server.uri(), &params)
            .await
            .expect_err("422 must fail immediately");
        assert!(!err.is_retryable());
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            }
        ));
    }
}
