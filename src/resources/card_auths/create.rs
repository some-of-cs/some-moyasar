//! Standalone 3DS card authentication creation (`CreateCardAuthParams`, `CardAuthsService::create`).

use core::fmt;

use http::header::CONTENT_TYPE;
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use zeroize::Zeroize;

use crate::kernel::auth::AuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::transport::send_with_retry;
use crate::kernel::types::{Amount, Currency};
use crate::resources::card_auths::CardAuthsService;
use crate::resources::card_auths::models::CardAuthentication;

/// Request parameters for creating a standalone 3D Secure card authentication (`POST /v1/card_auths`).
///
/// Sensitive cardholder data (`number` and `cvc`) are wrapped in [`secrecy::SecretString`],
/// zeroized on drop, and redacted as `"[REDACTED]"` in [`fmt::Debug`] output.
///
/// # Examples
///
/// ```
/// use secrecy::SecretString;
/// use some_moyasar::kernel::types::{Amount, Currency};
/// use some_moyasar::resources::card_auths::create::CreateCardAuthParams;
///
/// let params = CreateCardAuthParams::new(
///     Amount::from_major(100, Currency::SAR),
///     Currency::SAR,
///     "https://merchant.example/3ds/callback",
///     "Ahmed Ali",
///     SecretString::from("4111111111111111"),
///     SecretString::from("123"),
///     12,
///     2028,
/// )
/// .expect("valid HTTPS callback_url");
///
/// let debug = format!("{params:?}");
/// assert!(debug.contains("[REDACTED]"));
/// assert!(!debug.contains("4111111111111111"));
/// assert!(!debug.contains("123"));
/// ```
#[derive(Clone)]
pub struct CreateCardAuthParams {
    /// Transaction amount in minor currency units (e.g., Halalas for `SAR`).
    pub amount: Amount,
    /// ISO 4217 currency code.
    pub currency: Currency,
    /// HTTPS callback URL where Moyasar redirects after 3DS challenge completion.
    pub callback_url: String,
    /// Cardholder name as printed on the card.
    pub name: String,
    /// Primary account number (PAN), zeroized on drop and never logged.
    pub number: SecretString,
    /// Card verification code (CVC), zeroized on drop and never logged.
    pub cvc: SecretString,
    /// Card expiration month (`1..=12`).
    pub month: u8,
    /// Card expiration year (e.g., `2028`).
    pub year: u16,
    /// Optional human-readable description attached to the authentication.
    pub description: Option<String>,
}

impl CreateCardAuthParams {
    /// Constructs [`CreateCardAuthParams`] and validates that `callback_url` uses `https://`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        amount: Amount,
        currency: Currency,
        callback_url: impl Into<String>,
        name: impl Into<String>,
        number: SecretString,
        cvc: SecretString,
        month: u8,
        year: u16,
    ) -> Result<Self, MoyasarError> {
        let params = Self {
            amount,
            currency,
            callback_url: callback_url.into(),
            name: name.into(),
            number,
            cvc,
            month,
            year,
            description: None,
        };
        params.validate()?;
        Ok(params)
    }

    /// Attaches an optional description to the card authentication request.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Validates that `callback_url` uses the `https://` scheme with a non-empty host.
    pub fn validate(&self) -> Result<(), MoyasarError> {
        let rest = self
            .callback_url
            .strip_prefix("https://")
            .ok_or(MoyasarError::InvalidBaseUrl)?;
        if rest.is_empty() {
            return Err(MoyasarError::InvalidBaseUrl);
        }
        Ok(())
    }
}

impl fmt::Debug for CreateCardAuthParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateCardAuthParams")
            .field("amount", &self.amount)
            .field("currency", &self.currency)
            .field("callback_url", &self.callback_url)
            .field("name", &self.name)
            .field("number", &"[REDACTED]")
            .field("cvc", &"[REDACTED]")
            .field("month", &self.month)
            .field("year", &self.year)
            .field("description", &self.description)
            .finish()
    }
}

#[derive(Serialize)]
struct CreateCardAuthPayload<'a> {
    amount: Amount,
    currency: Currency,
    callback_url: &'a str,
    name: &'a str,
    number: &'a str,
    cvc: &'a str,
    month: u8,
    year: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
}

impl<'a> From<&'a CreateCardAuthParams> for CreateCardAuthPayload<'a> {
    fn from(params: &'a CreateCardAuthParams) -> Self {
        Self {
            amount: params.amount,
            currency: params.currency,
            callback_url: &params.callback_url,
            name: &params.name,
            number: params.number.expose_secret(),
            cvc: params.cvc.expose_secret(),
            month: params.month,
            year: params.year,
            description: params.description.as_deref(),
        }
    }
}

impl<'a, K: AuthKey> CardAuthsService<'a, K> {
    /// Initiates a standalone 3D Secure card authentication (`POST /v1/card_auths`).
    ///
    /// Validates `params.callback_url` (`https://` required) before dispatching the request
    /// with bounded retry handling for transient network and 5xx failures.
    ///
    /// # Examples
    ///
    /// ```
    /// use secrecy::SecretString;
    /// use some_moyasar::kernel::auth::SecretKey;
    /// use some_moyasar::kernel::client::MoyasarClient;
    /// use some_moyasar::kernel::types::{Amount, Currency};
    /// use some_moyasar::resources::card_auths::create::CreateCardAuthParams;
    ///
    /// let key = SecretKey::new("sk_test_example").unwrap();
    /// let client = MoyasarClient::new(key).unwrap();
    /// let service = client.card_auths();
    ///
    /// let invalid = CreateCardAuthParams::new(
    ///     Amount::from_major(50, Currency::SAR),
    ///     Currency::SAR,
    ///     "http://insecure.example/callback",
    ///     "Ahmed Ali",
    ///     SecretString::from("4111111111111111"),
    ///     SecretString::from("123"),
    ///     12,
    ///     2028,
    /// );
    /// assert!(invalid.is_err());
    /// assert_eq!(service.client().config().base_url, "https://api.moyasar.com");
    /// ```
    pub async fn create(
        &self,
        params: &CreateCardAuthParams,
    ) -> Result<CardAuthentication, MoyasarError> {
        params.validate()?;
        let base = self.client().config().base_url.trim_end_matches('/');
        let url = format!("{base}/v1/card_auths");
        let payload = CreateCardAuthPayload::from(params);
        let retries = self.client().config().max_network_retries;
        send_with_retry(retries, || self.send_create(&url, &payload)).await
    }

    async fn send_create(
        &self,
        url: &str,
        payload: &CreateCardAuthPayload<'_>,
    ) -> Result<CardAuthentication, MoyasarError> {
        let mut body_bytes =
            serde_json::to_vec(payload).map_err(|err| MoyasarError::ServerError {
                status: 500,
                message: format!("failed to serialize card_auth request: {err}"),
            })?;
        let send_res = self
            .client()
            .http()
            .post(url)
            .header(CONTENT_TYPE, "application/json")
            .body(body_bytes.clone())
            .send()
            .await;
        body_bytes.zeroize();
        let response = send_res.map_err(map_reqwest_error)?;
        let status = response.status();
        if status.is_success() {
            return response
                .json::<CardAuthentication>()
                .await
                .map_err(MoyasarError::Network);
        }
        let body = response.text().await.unwrap_or_default();
        Err(MoyasarError::from_api_response(status.as_u16(), &body))
    }
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
    use crate::resources::card_auths::models::CardAuthStatus;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sample_params() -> CreateCardAuthParams {
        CreateCardAuthParams::new(
            Amount::from_major(100, Currency::SAR),
            Currency::SAR,
            "https://merchant.example/3ds/callback",
            "Ahmed Ali",
            SecretString::from("4111111111111111"),
            SecretString::from("123"),
            12,
            2028,
        )
        .expect("valid sample params")
        .with_description("3DS pre-auth")
    }

    #[test]
    fn rejects_non_https_callback_urls() {
        for bad_url in ["http://merchant.example/cb", "https://", ""] {
            let err = CreateCardAuthParams::new(
                Amount::from_minor(5000),
                Currency::SAR,
                bad_url,
                "Ahmed Ali",
                SecretString::from("4111111111111111"),
                SecretString::from("123"),
                12,
                2028,
            )
            .expect_err("must reject non-HTTPS callback_url");
            assert!(matches!(err, MoyasarError::InvalidBaseUrl));
        }
    }

    #[test]
    fn redacts_pan_and_cvc_in_debug_output() {
        let params = sample_params();
        let debug = format!("{params:?}");
        assert!(debug.contains("number: \"[REDACTED]\""));
        assert!(debug.contains("cvc: \"[REDACTED]\""));
        assert!(!debug.contains("4111111111111111"));
        assert!(!debug.contains("123"));
    }

    #[tokio::test]
    async fn ac_us_4_3_0_01_tc_4_3_0_01_creates_card_auth_via_wiremock() {
        let server = MockServer::start().await;
        let response_body = serde_json::json!({
            "id": "ca_01h9x8z7y6w5",
            "status": "initiated",
            "transaction_url": "https://api.moyasar.com/v1/card_auth/ca_01h9x8z7y6w5/prepare",
            "result": null
        });

        Mock::given(method("POST"))
            .and(path("/v1/card_auths"))
            .and(header("moyasar-version", "2024-01-01"))
            .and(header("accept", "application/json"))
            .respond_with(ResponseTemplate::new(201).set_body_json(response_body))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_wiremock").expect("valid key");
        let client = MoyasarClient::new(key).expect("client builds");
        let service = client.card_auths();
        let params = sample_params();
        let payload = CreateCardAuthPayload::from(&params);
        let url = format!("{}/v1/card_auths", server.uri());

        let auth = service
            .send_create(&url, &payload)
            .await
            .expect("card auth creation succeeds");

        assert!(auth.id.to_string().starts_with("ca_"));
        assert_eq!(auth.status, CardAuthStatus::Initiated);
        let tx_url = auth
            .transaction_url
            .as_deref()
            .expect("transaction_url present");
        assert!(tx_url.contains("/v1/card_auth/"));
        assert!(tx_url.contains("/prepare"));
    }

    #[tokio::test]
    async fn maps_api_422_validation_error() {
        let server = MockServer::start().await;
        let error_body = serde_json::json!({
            "type": "invalid_request_error",
            "message": "Invalid card number",
            "errors": { "number": ["is invalid"] }
        });

        Mock::given(method("POST"))
            .and(path("/v1/card_auths"))
            .respond_with(ResponseTemplate::new(422).set_body_json(error_body))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_wiremock").expect("valid key");
        let client = MoyasarClient::new(key).expect("client builds");
        let service = client.card_auths();
        let params = sample_params();
        let payload = CreateCardAuthPayload::from(&params);
        let url = format!("{}/v1/card_auths", server.uri());

        let err = service
            .send_create(&url, &payload)
            .await
            .expect_err("422 must fail");

        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            }
        ));
    }
}
