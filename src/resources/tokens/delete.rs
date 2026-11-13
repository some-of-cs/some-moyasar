//! Card token deletion (`DELETE /v1/tokens/:id`) and token status gate helpers.

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::transport::execute_with_retry;
use crate::resources::tokens::TokensService;
use crate::resources::tokens::models::{TokenId, TokenStatus};

const TOKENS_PATH: &str = "/v1/tokens";

impl<'a, K: SecretAuthKey> TokensService<'a, K> {
    /// Deletes a card token (`DELETE /v1/tokens/{id}`), handling `204 No Content`
    /// with an empty response body as `Ok(())`.
    ///
    /// Requires a [`SecretAuthKey`] (`SecretKey`); publishable keys are rejected
    /// at compile time.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::Api`] for 4xx API errors (such as `404` when the
    /// token does not exist or `401` on invalid credentials), [`MoyasarError::ServerError`]
    /// for 5xx responses after retries are exhausted, or [`MoyasarError::Network`] /
    /// [`MoyasarError::Timeout`] on transport failures.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::kernel::auth::SecretKey;
    /// use some_moyasar::kernel::client::MoyasarClient;
    /// use some_moyasar::resources::tokens::models::TokenId;
    ///
    /// # async fn example() -> Result<(), some_moyasar::kernel::error::MoyasarError> {
    /// let key = SecretKey::new("sk_test_abc123")?;
    /// let client = MoyasarClient::new(key)?;
    /// let id = TokenId::new("token_abc123")?;
    /// client.tokens().delete(&id).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// A [`PublishableKey`](crate::kernel::auth::PublishableKey) cannot call `delete`:
    ///
    /// ```compile_fail
    /// use some_moyasar::kernel::auth::PublishableKey;
    /// use some_moyasar::kernel::client::MoyasarClient;
    /// use some_moyasar::resources::tokens::models::TokenId;
    ///
    /// async fn cannot_delete_with_publishable_key() {
    ///     let pk = PublishableKey::new("pk_test_abc123").unwrap();
    ///     let client = MoyasarClient::new(pk).unwrap();
    ///     let id = TokenId::new("token_abc123").unwrap();
    ///     let _ = client.tokens().delete(&id).await;
    /// }
    /// ```
    pub async fn delete(&self, id: &TokenId) -> Result<(), MoyasarError> {
        let client = self.client();
        delete_token(
            client.http(),
            &client.config().base_url,
            client.config().max_network_retries,
            id,
        )
        .await
    }
}

async fn delete_token(
    http: &reqwest::Client,
    base_url: &str,
    max_network_retries: u32,
    id: &TokenId,
) -> Result<(), MoyasarError> {
    let url = format!("{}{TOKENS_PATH}/{id}", base_url.trim_end_matches('/'));
    execute_with_retry(max_network_retries, || send_delete_once(http, &url)).await
}

async fn send_delete_once(http: &reqwest::Client, url: &str) -> Result<(), MoyasarError> {
    let response = http.delete(url).send().await.map_err(map_transport_error)?;
    let status = response.status();
    if status == reqwest::StatusCode::NO_CONTENT || status.is_success() {
        return Ok(());
    }
    let status_code = status.as_u16();
    let body = response.text().await.unwrap_or_default();
    Err(MoyasarError::from_api_response(status_code, &body))
}

fn map_transport_error(err: reqwest::Error) -> MoyasarError {
    if err.is_timeout() {
        MoyasarError::Timeout
    } else {
        MoyasarError::Network(err)
    }
}

/// Returns `true` when a token is in [`TokenStatus::Active`] and eligible for off-session charges.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::tokens::delete::is_token_chargeable;
/// use some_moyasar::resources::tokens::models::TokenStatus;
///
/// assert!(is_token_chargeable(TokenStatus::Active));
/// assert!(!is_token_chargeable(TokenStatus::Initiated));
/// assert!(!is_token_chargeable(TokenStatus::Inactive));
/// ```
#[must_use]
pub const fn is_token_chargeable(status: TokenStatus) -> bool {
    matches!(status, TokenStatus::Active)
}

/// Returns `true` when a token is in a non-terminal state ([`TokenStatus::Initiated`] or
/// [`TokenStatus::Active`]) and may be revoked via [`TokensService::delete`].
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::tokens::delete::is_token_deletable;
/// use some_moyasar::resources::tokens::models::TokenStatus;
///
/// assert!(is_token_deletable(TokenStatus::Initiated));
/// assert!(is_token_deletable(TokenStatus::Active));
/// assert!(!is_token_deletable(TokenStatus::Inactive));
/// ```
#[must_use]
pub const fn is_token_deletable(status: TokenStatus) -> bool {
    matches!(status, TokenStatus::Initiated | TokenStatus::Active)
}

/// Enforces the Active-Before-Charge invariant, returning `Ok(())` only when `status` is
/// [`TokenStatus::Active`].
///
/// # Errors
///
/// Returns [`MoyasarError::Api`] with [`ApiErrorType::InvalidRequestError`] when the token
/// is still [`TokenStatus::Initiated`] or has transitioned to [`TokenStatus::Inactive`].
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::tokens::delete::require_active_token;
/// use some_moyasar::resources::tokens::models::TokenStatus;
///
/// assert!(require_active_token(TokenStatus::Active).is_ok());
/// assert!(require_active_token(TokenStatus::Initiated).is_err());
/// assert!(require_active_token(TokenStatus::Inactive).is_err());
/// ```
pub fn require_active_token(status: TokenStatus) -> Result<(), MoyasarError> {
    match status {
        TokenStatus::Active => Ok(()),
        TokenStatus::Initiated => Err(MoyasarError::Api {
            error_type: ApiErrorType::InvalidRequestError,
            message: "token is still initiated and has not completed verification".to_owned(),
            errors: Vec::new(),
            response_code: None,
        }),
        TokenStatus::Inactive | TokenStatus::Unknown => Err(MoyasarError::Api {
            error_type: ApiErrorType::InvalidRequestError,
            message: "token is inactive and cannot be charged".to_owned(),
            errors: Vec::new(),
            response_code: None,
        }),
    }
}

/// Returns `true` if transitioning from `from` to `to` is valid in the token lifecycle
/// (`initiated -> active | inactive`, `active -> inactive`).
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::tokens::delete::can_transition_token_status;
/// use some_moyasar::resources::tokens::models::TokenStatus;
///
/// assert!(can_transition_token_status(TokenStatus::Initiated, TokenStatus::Active));
/// assert!(can_transition_token_status(TokenStatus::Initiated, TokenStatus::Inactive));
/// assert!(can_transition_token_status(TokenStatus::Active, TokenStatus::Inactive));
/// assert!(!can_transition_token_status(TokenStatus::Inactive, TokenStatus::Active));
/// ```
#[must_use]
pub const fn can_transition_token_status(from: TokenStatus, to: TokenStatus) -> bool {
    matches!(
        (from, to),
        (
            TokenStatus::Initiated,
            TokenStatus::Active | TokenStatus::Inactive
        ) | (TokenStatus::Active, TokenStatus::Inactive)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::client::MoyasarClient;
    use crate::kernel::transport::{API_VERSION, MOYASAR_VERSION_HEADER, build_default_headers};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_http_client(key: &SecretKey) -> reqwest::Client {
        let headers = build_default_headers(key).expect("default headers build");
        reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .expect("reqwest client builds")
    }

    #[tokio::test]
    async fn ac_us_4_1_0_04_tc_4_1_0_04_delete_handles_204_empty_body_as_ok() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/v1/tokens/token_abc123"))
            .and(header(MOYASAR_VERSION_HEADER, API_VERSION))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_delete_204").expect("valid secret key");
        let http = test_http_client(&key);
        let id = TokenId::new("token_abc123").expect("valid token id");

        let result = delete_token(&http, &server.uri(), 0, &id).await;
        assert!(
            result.is_ok(),
            "expected Ok(()) for 204 No Content, got {result:?}"
        );
    }

    #[tokio::test]
    async fn tc_4_1_0_04_delete_maps_404_and_401_without_retrying() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/v1/tokens/token_missing"))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_string(r#"{"type":"record_not_found","message":"Token not found"}"#),
            )
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("DELETE"))
            .and(path("/v1/tokens/token_unauth"))
            .respond_with(
                ResponseTemplate::new(401)
                    .set_body_string(r#"{"type":"authentication_error","message":"Invalid key"}"#),
            )
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_delete_err").expect("valid secret key");
        let http = test_http_client(&key);

        let missing = TokenId::new("token_missing").expect("valid token id");
        let err_404 = delete_token(&http, &server.uri(), 2, &missing)
            .await
            .expect_err("404 must return error");
        assert!(matches!(
            err_404,
            MoyasarError::Api {
                error_type: ApiErrorType::RecordNotFound,
                ..
            }
        ));

        let unauth = TokenId::new("token_unauth").expect("valid token id");
        let err_401 = delete_token(&http, &server.uri(), 2, &unauth)
            .await
            .expect_err("401 must return error");
        assert!(matches!(
            err_401,
            MoyasarError::Api {
                error_type: ApiErrorType::AuthenticationError,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn delete_retries_transient_503_then_succeeds_on_204() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/v1/tokens/token_retry"))
            .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("DELETE"))
            .and(path("/v1/tokens/token_retry"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_delete_retry").expect("valid secret key");
        let http = test_http_client(&key);
        let id = TokenId::new("token_retry").expect("valid token id");

        let result = delete_token(&http, &server.uri(), 2, &id).await;
        assert!(
            result.is_ok(),
            "expected retry recovery on 204, got {result:?}"
        );
    }

    #[test]
    fn token_status_gates_and_service_handle_work_as_specified() {
        let key = SecretKey::new("sk_test_service_handle").expect("valid secret key");
        let client = MoyasarClient::new(key).expect("client builds");
        let _service = client.tokens();

        assert!(is_token_chargeable(TokenStatus::Active));
        assert!(!is_token_chargeable(TokenStatus::Initiated));
        assert!(!is_token_chargeable(TokenStatus::Inactive));

        assert!(is_token_deletable(TokenStatus::Initiated));
        assert!(is_token_deletable(TokenStatus::Active));
        assert!(!is_token_deletable(TokenStatus::Inactive));

        assert!(require_active_token(TokenStatus::Active).is_ok());
        assert!(matches!(
            require_active_token(TokenStatus::Initiated),
            Err(MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            })
        ));
        assert!(matches!(
            require_active_token(TokenStatus::Inactive),
            Err(MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            })
        ));

        assert!(can_transition_token_status(
            TokenStatus::Initiated,
            TokenStatus::Active
        ));
        assert!(can_transition_token_status(
            TokenStatus::Initiated,
            TokenStatus::Inactive
        ));
        assert!(can_transition_token_status(
            TokenStatus::Active,
            TokenStatus::Inactive
        ));
        assert!(!can_transition_token_status(
            TokenStatus::Inactive,
            TokenStatus::Active
        ));
    }
}
