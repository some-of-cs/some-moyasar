//! Card token creation endpoint (`POST /v1/tokens`).

use core::fmt;

use secrecy::{ExposeSecret, SecretString};
use serde::{Serialize, Serializer};

use crate::kernel::auth::AuthKey;
use crate::kernel::client::MoyasarClient;
use crate::kernel::error::MoyasarError;
use crate::kernel::metadata::Metadata;
use crate::kernel::transport::send_with_retry;
use crate::resources::tokens::TokensService;
use crate::resources::tokens::models::Token;

const TOKENS_PATH: &str = "/v1/tokens";

fn serialize_secret<S: Serializer>(
    secret: &SecretString,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(secret.expose_secret())
}

/// Request payload for creating a Moyasar card token (`POST /v1/tokens`).
///
/// Sensitive cardholder fields (`number` and `cvc`) are stored in
/// [`secrecy::SecretString`] buffers that zeroize on drop and print
/// `[REDACTED]` in [`fmt::Debug`] output.
///
/// # Examples
///
/// ```
/// use some_moyasar::kernel::metadata::Metadata;
/// use some_moyasar::resources::tokens::create::CreateTokenParams;
///
/// let mut meta = Metadata::new();
/// meta.insert("order_id", "ord_42").unwrap();
///
/// let params = CreateTokenParams::new("Amina Ali", "4111111111111111", "08", "2029", "123")
///     .save_only(true)
///     .callback_url("https://merchant.example/3ds/callback")
///     .metadata(meta);
///
/// let debug_output = format!("{params:?}");
/// assert!(debug_output.contains("[REDACTED]"));
/// assert!(!debug_output.contains("4111111111111111"));
/// assert!(!debug_output.contains("123"));
/// ```
#[derive(Clone, Serialize)]
pub struct CreateTokenParams {
    /// Cardholder name as printed on the card.
    pub name: String,
    #[serde(serialize_with = "serialize_secret")]
    number: SecretString,
    /// Two-digit expiration month (e.g., `"08"`).
    pub month: String,
    /// Two- or four-digit expiration year (e.g., `"2029"`).
    pub year: String,
    #[serde(serialize_with = "serialize_secret")]
    cvc: SecretString,
    /// Optional redirect URL after 3D Secure verification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_url: Option<String>,
    /// When `true`, saves the token without immediate verification (`save_only`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save_only: Option<bool>,
    /// Optional key/value metadata attached to the token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

impl CreateTokenParams {
    /// Creates a new [`CreateTokenParams`] payload with required card details.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        number: impl Into<Box<str>>,
        month: impl Into<String>,
        year: impl Into<String>,
        cvc: impl Into<Box<str>>,
    ) -> Self {
        Self {
            name: name.into(),
            number: SecretString::from(number.into()),
            month: month.into(),
            year: year.into(),
            cvc: SecretString::from(cvc.into()),
            callback_url: None,
            save_only: None,
            metadata: None,
        }
    }

    /// Sets the optional redirect URL for 3D Secure verification.
    #[must_use]
    pub fn callback_url(mut self, url: impl Into<String>) -> Self {
        self.callback_url = Some(url.into());
        self
    }

    /// Sets whether cardholder verification should be deferred (`save_only`).
    #[must_use]
    pub const fn save_only(mut self, save_only: bool) -> Self {
        self.save_only = Some(save_only);
        self
    }

    /// Attaches optional key/value [`Metadata`] to the token.
    #[must_use]
    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

impl fmt::Debug for CreateTokenParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateTokenParams")
            .field("name", &self.name)
            .field("number", &"[REDACTED]")
            .field("month", &self.month)
            .field("year", &self.year)
            .field("cvc", &"[REDACTED]")
            .field("callback_url", &self.callback_url)
            .field("save_only", &self.save_only)
            .field("metadata", &self.metadata)
            .finish()
    }
}

impl<'a, K: AuthKey> TokensService<'a, K> {
    /// Creates a card token via `POST /v1/tokens` using either a publishable or secret key.
    ///
    /// Transient network, timeout, and 5xx failures are retried automatically according to
    /// [`MoyasarClientConfig::max_network_retries`](crate::kernel::client::MoyasarClientConfig::max_network_retries).
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::Api`] on 4xx API errors (such as validation or authentication
    /// failures), [`MoyasarError::ServerError`] on exhausted 5xx responses,
    /// [`MoyasarError::Timeout`] on request timeouts, or [`MoyasarError::Network`] on
    /// transport failures.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::tokens::create::CreateTokenParams;
    ///
    /// # async fn run() -> Result<(), some_moyasar::prelude::MoyasarError> {
    /// let pk = PublishableKey::new("pk_test_your_key")?;
    /// let client = MoyasarClient::new(pk)?;
    /// let params = CreateTokenParams::new("Amina Ali", "4111111111111111", "08", "2029", "123")
    ///     .save_only(true);
    ///
    /// let token = client.tokens().create(&params).await?;
    /// println!("created token: {}", token.id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create(&self, params: &CreateTokenParams) -> Result<Token, MoyasarError> {
        let client = self.client();
        post_create_token(client, &client.config().base_url, params).await
    }
}

async fn post_create_token<K: AuthKey>(
    client: &MoyasarClient<K>,
    base_url: &str,
    params: &CreateTokenParams,
) -> Result<Token, MoyasarError> {
    let url = format!("{}{}", base_url.trim_end_matches('/'), TOKENS_PATH);
    let max_retries = client.config().max_network_retries;
    let (status, body) = send_with_retry(max_retries, || async {
        dispatch_post(client.http(), &url, params).await
    })
    .await?;
    parse_token_response(status, &body)
}

async fn dispatch_post(
    http: &reqwest::Client,
    url: &str,
    params: &CreateTokenParams,
) -> Result<(u16, String), MoyasarError> {
    let response = http
        .post(url)
        .json(params)
        .send()
        .await
        .map_err(map_reqwest_error)?;
    let status = response.status().as_u16();
    let body = response.text().await.map_err(map_reqwest_error)?;
    if status >= 500 {
        return Err(MoyasarError::from_api_response(status, &body));
    }
    Ok((status, body))
}

fn map_reqwest_error(err: reqwest::Error) -> MoyasarError {
    if err.is_timeout() {
        MoyasarError::Timeout
    } else {
        MoyasarError::Network(err)
    }
}

fn parse_token_response(status: u16, body: &str) -> Result<Token, MoyasarError> {
    if !(200..300).contains(&status) {
        return Err(MoyasarError::from_api_response(status, body));
    }
    serde_json::from_str::<Token>(body).map_err(|err| MoyasarError::ServerError {
        status,
        message: format!("failed to decode token response: {err}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::{PublishableKey, SecretKey};
    use crate::kernel::error::ApiErrorType;
    use crate::kernel::transport::{API_VERSION, MOYASAR_VERSION_HEADER};
    use crate::resources::tokens::models::{TokenId, TokenStatus};
    use std::sync::atomic::{AtomicU32, Ordering};
    use wiremock::matchers::{header, header_exists, method, path};
    use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

    fn sample_token_json(id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "status": "initiated",
            "number": "XXXX-XXXX-XXXX-1111",
            "company": "visa",
            "gateway_id": "gw_test_123",
            "created_at": "2026-11-13T10:00:00Z"
        })
    }

    #[test]
    fn debug_redacts_pan_and_cvc_and_serializes_optional_fields() {
        let mut meta = Metadata::new();
        meta.insert("order_id", "ord_99").expect("valid metadata");

        let minimal = CreateTokenParams::new("Amina Ali", "4111111111111111", "08", "2029", "987");
        let dbg = format!("{minimal:?}");
        assert!(dbg.contains("[REDACTED]"));
        assert!(!dbg.contains("4111111111111111"));
        assert!(!dbg.contains("987"));

        let min_json = serde_json::to_value(&minimal).expect("serialize minimal");
        assert!(min_json.get("callback_url").is_none());
        assert!(min_json.get("save_only").is_none());
        assert!(min_json.get("metadata").is_none());

        let full = minimal
            .save_only(true)
            .callback_url("https://merchant.example/cb")
            .metadata(meta);
        let full_json = serde_json::to_value(&full).expect("serialize full");
        assert_eq!(full_json["save_only"], true);
        assert_eq!(full_json["callback_url"], "https://merchant.example/cb");
        assert_eq!(full_json["metadata"]["order_id"], "ord_99");
        assert_eq!(full_json["number"], "4111111111111111");
        assert_eq!(full_json["cvc"], "987");
    }

    #[tokio::test]
    async fn ac_us_4_1_0_01_creates_token_with_publishable_key() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/tokens"))
            .and(header_exists("authorization"))
            .and(header(MOYASAR_VERSION_HEADER, API_VERSION))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(sample_token_json("token_pk_123")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let pk = PublishableKey::new("pk_test_unit2_pub").expect("valid publishable key");
        let client = MoyasarClient::new(pk).expect("client builds");
        let params = CreateTokenParams::new("Amina Ali", "4111111111111111", "08", "2029", "123");

        let token = post_create_token(&client, &server.uri(), &params)
            .await
            .expect("token created");
        assert!(token.id.to_string().starts_with("token_"));
        assert_eq!(
            token.id,
            TokenId::new("token_pk_123").expect("valid token id")
        );
        assert_eq!(token.status, TokenStatus::Initiated);
    }

    #[tokio::test]
    async fn ac_us_4_1_0_02_save_only_defers_verification() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/tokens"))
            .and(wiremock::matchers::body_partial_json(serde_json::json!({
                "save_only": true,
                "callback_url": "https://merchant.example/3ds"
            })))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(sample_token_json("token_save_only_1")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let pk = PublishableKey::new("pk_test_save_only").expect("valid key");
        let client = MoyasarClient::new(pk).expect("client builds");
        let params = CreateTokenParams::new("Amina Ali", "4111111111111111", "08", "2029", "123")
            .save_only(true)
            .callback_url("https://merchant.example/3ds");

        let token = post_create_token(&client, &server.uri(), &params)
            .await
            .expect("token created with save_only");
        assert_eq!(token.status, TokenStatus::Initiated);
    }

    #[tokio::test]
    async fn accepts_secret_key_for_token_creation() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/tokens"))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(sample_token_json("token_sk_456")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let sk = SecretKey::new("sk_test_unit2_sec").expect("valid secret key");
        let client = MoyasarClient::new(sk).expect("client builds");
        let params = CreateTokenParams::new("Amina Ali", "4111111111111111", "08", "2029", "123");

        let token = post_create_token(&client, &server.uri(), &params)
            .await
            .expect("token created with secret key");
        assert_eq!(token.status, TokenStatus::Initiated);
    }

    struct FlakyResponder {
        attempts: AtomicU32,
    }

    impl Respond for FlakyResponder {
        fn respond(&self, _request: &Request) -> ResponseTemplate {
            let attempt = self.attempts.fetch_add(1, Ordering::SeqCst);
            if attempt == 0 {
                ResponseTemplate::new(503).set_body_string("Service Unavailable")
            } else {
                ResponseTemplate::new(201).set_body_json(sample_token_json("token_retry_ok"))
            }
        }
    }

    #[tokio::test]
    async fn retries_transient_5xx_and_fails_fast_on_422() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/tokens"))
            .respond_with(FlakyResponder {
                attempts: AtomicU32::new(0),
            })
            .expect(2)
            .mount(&server)
            .await;

        let pk = PublishableKey::new("pk_test_retry").expect("valid key");
        let client = MoyasarClient::new(pk).expect("client builds");
        let params = CreateTokenParams::new("Amina Ali", "4111111111111111", "08", "2029", "123");

        let token = post_create_token(&client, &server.uri(), &params)
            .await
            .expect("recovers after 503");
        assert_eq!(token.id.to_string(), "token_retry_ok");

        let err_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/tokens"))
            .respond_with(ResponseTemplate::new(422).set_body_json(serde_json::json!({
                "type": "invalid_request_error",
                "message": "Invalid card number"
            })))
            .expect(1)
            .mount(&err_server)
            .await;

        let err = post_create_token(&client, &err_server.uri(), &params)
            .await
            .expect_err("422 must fail immediately");
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            }
        ));
    }
}
