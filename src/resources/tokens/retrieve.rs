//! Card token retrieval (`GET /v1/tokens/{id}`).

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::transport::execute_with_retry;
use crate::resources::tokens::TokensService;
use crate::resources::tokens::models::{Token, TokenId};

const TOKENS_PATH: &str = "/v1/tokens";

impl<'a, K: SecretAuthKey> TokensService<'a, K> {
    /// Retrieves a card token by its identifier (`GET /v1/tokens/{id}`).
    ///
    /// Requires a [`SecretAuthKey`] (such as [`SecretKey`](crate::kernel::auth::SecretKey));
    /// publishable keys are rejected at compile time. Returns masked card details
    /// suitable for customer display without exposing a full PAN (`AC-US-4.1.0-03`).
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::Api`] with
    /// [`ApiErrorType::RecordNotFound`](crate::kernel::error::ApiErrorType::RecordNotFound)
    /// when the token does not exist (HTTP 404), or a transport/server error on failure.
    ///
    /// # Examples
    ///
    /// Constructing a secret-key `TokensService` handle:
    ///
    /// ```
    /// use some_moyasar::prelude::{AuthKey, AuthMode, MoyasarClient, SecretKey};
    ///
    /// let key = SecretKey::new("sk_test_abc123").unwrap();
    /// let client = MoyasarClient::new(key).unwrap();
    /// let tokens = client.tokens();
    /// assert_eq!(tokens.client().key().mode(), AuthMode::Test);
    /// ```
    ///
    /// Retrieving an active token asynchronously:
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    /// use some_moyasar::resources::tokens::models::TokenId;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let key = SecretKey::new("sk_test_abc123")?;
    /// let client = MoyasarClient::new(key)?;
    /// let id = TokenId::new("token_6b9b4f21a1e24d10")?;
    /// let token = client.tokens().retrieve(&id).await?;
    /// assert!(token.number.starts_with("XXXX-XXXX-XXXX-"));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// A [`PublishableKey`](crate::kernel::auth::PublishableKey) cannot call `retrieve()`:
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::tokens::models::TokenId;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let pk = PublishableKey::new("pk_test_abc123")?;
    /// let client = MoyasarClient::new(pk)?;
    /// let id = TokenId::new("token_6b9b4f21a1e24d10")?;
    /// let _token = client.tokens().retrieve(&id).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn retrieve(&self, id: &TokenId) -> Result<Token, MoyasarError> {
        let client = self.client();
        retrieve_token(
            client.http(),
            &client.config().base_url,
            client.config().max_network_retries,
            id,
        )
        .await
    }
}

async fn retrieve_token(
    http: &reqwest::Client,
    base_url: &str,
    max_network_retries: u32,
    id: &TokenId,
) -> Result<Token, MoyasarError> {
    let url = format!("{}{TOKENS_PATH}/{id}", base_url.trim_end_matches('/'));
    execute_with_retry(max_network_retries, || send_retrieve_request(http, &url)).await
}

async fn send_retrieve_request(
    http: &reqwest::Client,
    url: &str,
) -> Result<Token, MoyasarError> {
    let response = http.get(url).send().await.map_err(map_reqwest_error)?;
    let status = response.status();
    if status.is_success() {
        return response.json::<Token>().await.map_err(map_reqwest_error);
    }
    let body = response.text().await.unwrap_or_default();
    Err(MoyasarError::from_api_response(status.as_u16(), &body))
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
    use crate::resources::tokens::models::TokenStatus;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn ac_us_4_1_0_03_retrieves_masked_token_with_secret_key() {
        let server = MockServer::start().await;
        let token_id = TokenId::new("token_6b9b4f21a1e24d10").expect("valid token id");

        Mock::given(method("GET"))
            .and(path("/v1/tokens/token_6b9b4f21a1e24d10"))
            .and(header("moyasar-version", "2024-01-01"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "token_6b9b4f21a1e24d10",
                "status": "active",
                "number": "XXXX-XXXX-XXXX-1111",
                "company": "visa",
                "gateway_id": "gw_123456",
                "created_at": "2026-01-15T10:30:00Z"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_abc123").expect("valid secret key");
        let client = MoyasarClient::builder(key)
            .max_network_retries(0)
            .build()
            .expect("client builds");

        let token = retrieve_token(
            client.http(),
            &server.uri(),
            client.config().max_network_retries,
            &token_id,
        )
        .await
        .expect("token retrieval succeeds");

        assert_eq!(token.id, token_id);
        assert_eq!(token.status, TokenStatus::Active);
        assert_eq!(token.number, "XXXX-XXXX-XXXX-1111");
        assert_eq!(token.company.as_deref(), Some("visa"));
        assert!(!format!("{token:?}").contains("4111111111111111"));
    }

    #[tokio::test]
    async fn maps_404_record_not_found_to_moyasar_error() {
        let server = MockServer::start().await;
        let token_id = TokenId::new("token_missing_404").expect("valid token id");

        Mock::given(method("GET"))
            .and(path("/v1/tokens/token_missing_404"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "type": "record_not_found",
                "message": "Token not found"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_abc123").expect("valid secret key");
        let client = MoyasarClient::builder(key)
            .max_network_retries(2)
            .build()
            .expect("client builds");

        let err = retrieve_token(
            client.http(),
            &server.uri(),
            client.config().max_network_retries,
            &token_id,
        )
        .await
        .expect_err("404 must return RecordNotFound");

        assert!(!err.is_retryable());
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::RecordNotFound,
                ref message,
                ..
            } if message == "Token not found"
        ));
    }

    #[tokio::test]
    async fn retries_transient_503_before_succeeding() {
        let server = MockServer::start().await;
        let token_id = TokenId::new("token_retry_503").expect("valid token id");

        Mock::given(method("GET"))
            .and(path("/v1/tokens/token_retry_503"))
            .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/tokens/token_retry_503"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "token_retry_503",
                "status": "active",
                "number": "XXXX-XXXX-XXXX-4242",
                "company": "master",
                "gateway_id": null,
                "created_at": "2026-01-15T10:30:00Z"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_abc123").expect("valid secret key");
        let client = MoyasarClient::builder(key)
            .max_network_retries(2)
            .build()
            .expect("client builds");

        let token = retrieve_token(
            client.http(),
            &server.uri(),
            client.config().max_network_retries,
            &token_id,
        )
        .await
        .expect("retries 503 and succeeds");

        assert_eq!(token.id, token_id);
        assert_eq!(token.status, TokenStatus::Active);
        assert_eq!(token.number, "XXXX-XXXX-XXXX-4242");
    }
}
