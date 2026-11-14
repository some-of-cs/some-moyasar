//! Card BIN/IIN issuer lookup types and validation (US-7.1.0 Unit 1).

use core::fmt;

use http::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};

use crate::kernel::auth::AuthKey;
use crate::kernel::client::MoyasarClient;
use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::response_code::FieldError;
use crate::kernel::transport::execute_with_retry;

/// Card funding classification returned by the Moyasar BIN/IIN issuer lookup endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssuerCardType {
    /// Debit card (for example, local `mada` debit cards).
    Debit,
    /// Credit card.
    Credit,
    /// Charge card.
    ChargeCard,
    /// Card funding type was not specified by the issuer.
    Unspecified,
    /// Forward-compatible fallback for unrecognized card types.
    #[serde(other)]
    Unknown,
}

/// Card issuer metadata returned by `POST /v1/source/issuer`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct IssuerMetadata {
    /// Issuing bank or financial institution name, if available.
    pub issuer_name: Option<String>,
    /// Two-letter ISO 3166-1 alpha-2 country code of the issuer (e.g., `"SA"`).
    pub issuer_country: Option<String>,
    /// Funding classification of the card (`Debit`, `Credit`, `ChargeCard`, etc.).
    pub issuer_card_type: IssuerCardType,
    /// Card program or tier category (e.g., `"classic"`, `"platinum"`), if available.
    pub issuer_card_category: Option<String>,
    /// Leading BIN/IIN digits echoed by the API.
    pub first_digits: String,
    /// Trailing card digits, if returned by the API.
    pub last_digits: Option<String>,
}

/// Validated request parameters for looking up a card issuer by BIN/IIN (`POST /v1/source/issuer`).
///
/// Enforces PCI-DSS data minimization by accepting only 6 to 8 ASCII digits and
/// redacting `first_digits` as `"[REDACTED]"` in [`fmt::Debug`] output.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::sources::issuer::BinLookupParams;
///
/// let params = BinLookupParams::new("420132").expect("valid 6-digit BIN");
/// assert_eq!(params.first_digits(), "420132");
///
/// let debug = format!("{params:?}");
/// assert!(debug.contains("[REDACTED]"));
/// assert!(!debug.contains("420132"));
///
/// // Inputs shorter than 6 digits, longer than 8 digits (including full PANs), or non-digits fail.
/// assert!(BinLookupParams::new("1234").is_err());
/// assert!(BinLookupParams::new("4111111111111111").is_err());
/// assert!(BinLookupParams::new("42013a").is_err());
/// ```
#[derive(Clone, Serialize)]
pub struct BinLookupParams {
    first_digits: String,
}

impl BinLookupParams {
    /// Constructs [`BinLookupParams`] after validating that `first_digits` contains 6 to 8 ASCII digits.
    pub fn new(first_digits: impl Into<String>) -> Result<Self, MoyasarError> {
        let digits = first_digits.into();
        validate_bin(&digits)?;
        Ok(Self {
            first_digits: digits,
        })
    }

    /// Returns the validated 6-to-8-digit BIN/IIN prefix.
    pub fn first_digits(&self) -> &str {
        &self.first_digits
    }
}

impl fmt::Debug for BinLookupParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BinLookupParams")
            .field("first_digits", &"[REDACTED]")
            .finish()
    }
}

fn validate_bin(first_digits: &str) -> Result<(), MoyasarError> {
    let valid_len = (6..=8).contains(&first_digits.len());
    let valid_digits = first_digits.bytes().all(|byte| byte.is_ascii_digit());
    if !valid_len || !valid_digits {
        return Err(MoyasarError::Api {
            error_type: ApiErrorType::InvalidRequestError,
            message: "first_digits must contain 6 to 8 ASCII digits".to_owned(),
            errors: vec![FieldError {
                field: "first_digits".to_owned(),
                message: "must be 6 to 8 ASCII digits".to_owned(),
            }],
            response_code: None,
        });
    }
    Ok(())
}

/// Dispatches `POST /v1/source/issuer` with bounded retry handling for transient failures.
#[allow(dead_code)]
pub(crate) async fn get_issuer_request<K: AuthKey>(
    client: &MoyasarClient<K>,
    params: &BinLookupParams,
) -> Result<IssuerMetadata, MoyasarError> {
    validate_bin(&params.first_digits)?;
    let base = client.config().base_url.trim_end_matches('/');
    let url = format!("{base}/v1/source/issuer");
    let retries = client.config().max_network_retries;
    execute_with_retry(retries, || send_issuer_request(client, &url, params)).await
}

#[allow(dead_code)]
async fn send_issuer_request<K: AuthKey>(
    client: &MoyasarClient<K>,
    url: &str,
    params: &BinLookupParams,
) -> Result<IssuerMetadata, MoyasarError> {
    let response = client
        .http()
        .post(url)
        .header(CONTENT_TYPE, "application/json")
        .json(params)
        .send()
        .await
        .map_err(map_reqwest_error)?;
    let status = response.status();
    if status.is_success() {
        return response
            .json::<IssuerMetadata>()
            .await
            .map_err(MoyasarError::Network);
    }
    let body = response.text().await.unwrap_or_default();
    Err(MoyasarError::from_api_response(status.as_u16(), &body))
}

#[allow(dead_code)]
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
    use crate::kernel::auth::{PublishableKey, SecretKey};
    use std::sync::atomic::{AtomicU32, Ordering};
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn tc_7_1_0_02_rejects_invalid_bins_pre_transport() {
        for invalid in [
            "",
            "1234",
            "42013",
            "420132001",
            "4111111111111111",
            "42013a",
            "4201 32",
            "4201-32",
        ] {
            let err = BinLookupParams::new(invalid).expect_err("invalid BIN must be rejected");
            assert!(matches!(
                err,
                MoyasarError::Api {
                    error_type: ApiErrorType::InvalidRequestError,
                    ..
                }
            ));
            assert!(!err.is_retryable());
        }
    }

    #[test]
    fn accepts_6_to_8_digit_bins_and_redacts_debug() {
        for valid in ["420132", "4201320", "42013200"] {
            let params = BinLookupParams::new(valid).expect("valid BIN");
            assert_eq!(params.first_digits(), valid);
            let debug = format!("{params:?}");
            assert!(debug.contains("\"[REDACTED]\""));
            assert!(!debug.contains(valid));
        }
    }

    #[test]
    fn deserializes_issuer_card_types_with_unknown_fallback() {
        assert_eq!(
            serde_json::from_str::<IssuerCardType>("\"debit\"").unwrap(),
            IssuerCardType::Debit
        );
        assert_eq!(
            serde_json::from_str::<IssuerCardType>("\"credit\"").unwrap(),
            IssuerCardType::Credit
        );
        assert_eq!(
            serde_json::from_str::<IssuerCardType>("\"charge_card\"").unwrap(),
            IssuerCardType::ChargeCard
        );
        assert_eq!(
            serde_json::from_str::<IssuerCardType>("\"unspecified\"").unwrap(),
            IssuerCardType::Unspecified
        );
        assert_eq!(
            serde_json::from_str::<IssuerCardType>("\"prepaid_future\"").unwrap(),
            IssuerCardType::Unknown
        );
    }

    #[tokio::test]
    async fn tc_7_1_0_01_looks_up_6_and_8_digit_bins_via_wiremock() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/source/issuer"))
            .and(header("moyasar-version", "2024-01-01"))
            .and(header("accept", "application/json"))
            .and(body_json(serde_json::json!({ "first_digits": "420132" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issuer_name": "Al Rajhi Bank",
                "issuer_country": "SA",
                "issuer_card_type": "debit",
                "issuer_card_category": "mada",
                "first_digits": "420132",
                "last_digits": null
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/source/issuer"))
            .and(body_json(serde_json::json!({ "first_digits": "42013200" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "issuer_name": "Saudi National Bank",
                "issuer_country": "SA",
                "issuer_card_type": "credit",
                "issuer_card_category": "platinum",
                "first_digits": "42013200",
                "last_digits": "1111"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let sk_client = MoyasarClient::new(SecretKey::new("sk_test_wiremock").expect("valid key"))
            .expect("client builds");
        let pk_client =
            MoyasarClient::new(PublishableKey::new("pk_test_wiremock").expect("valid key"))
                .expect("client builds");

        let url = format!("{}/v1/source/issuer", server.uri());

        let mada = send_issuer_request(
            &sk_client,
            &url,
            &BinLookupParams::new("420132").expect("valid 6-digit BIN"),
        )
        .await
        .expect("6-digit BIN lookup succeeds");
        assert_eq!(mada.issuer_country.as_deref(), Some("SA"));
        assert_eq!(mada.issuer_card_type, IssuerCardType::Debit);
        assert_eq!(mada.issuer_name.as_deref(), Some("Al Rajhi Bank"));

        let credit = send_issuer_request(
            &pk_client,
            &url,
            &BinLookupParams::new("42013200").expect("valid 8-digit BIN"),
        )
        .await
        .expect("8-digit BIN lookup succeeds");
        assert_eq!(credit.issuer_card_type, IssuerCardType::Credit);
        assert_eq!(credit.last_digits.as_deref(), Some("1111"));
    }

    #[tokio::test]
    async fn retries_5xx_and_surfaces_422_without_retry() {
        let server = MockServer::start().await;
        let attempts = AtomicU32::new(0);

        Mock::given(method("POST"))
            .and(path("/v1/source/issuer"))
            .respond_with(move |_req: &wiremock::Request| {
                let n = attempts.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    ResponseTemplate::new(503).set_body_string("Service Unavailable")
                } else {
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "issuer_name": "Riyad Bank",
                        "issuer_country": "SA",
                        "issuer_card_type": "debit",
                        "issuer_card_category": null,
                        "first_digits": "420132",
                        "last_digits": null
                    }))
                }
            })
            .expect(2)
            .mount(&server)
            .await;

        let client = MoyasarClient::new(SecretKey::new("sk_test_wiremock").expect("valid key"))
            .expect("client builds");
        let url = format!("{}/v1/source/issuer", server.uri());
        let params = BinLookupParams::new("420132").expect("valid BIN");

        let meta = execute_with_retry(2, || send_issuer_request(&client, &url, &params))
            .await
            .expect("recovers after 503");
        assert_eq!(meta.issuer_name.as_deref(), Some("Riyad Bank"));

        let server_422 = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/source/issuer"))
            .respond_with(ResponseTemplate::new(422).set_body_json(serde_json::json!({
                "type": "invalid_request_error",
                "message": "Invalid BIN prefix"
            })))
            .expect(1)
            .mount(&server_422)
            .await;

        let url_422 = format!("{}/v1/source/issuer", server_422.uri());
        let err = execute_with_retry(2, || send_issuer_request(&client, &url_422, &params))
            .await
            .expect_err("422 must not succeed or retry");
        assert!(matches!(err, MoyasarError::Api { .. }));
        assert!(!err.is_retryable());
    }
}
