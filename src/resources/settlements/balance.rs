//! Merchant balance (`MerchantBalance`, `SettlementsService::fetch_balance`).

use serde::{Deserialize, Deserializer, Serialize};

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::transport::execute_with_retry;
use crate::kernel::types::{Amount, Currency};
use crate::resources::settlements::service::SettlementsService;

/// Merchant settlement balance returned by `GET /v1/settlements/balance`.
///
/// Supports deserializing both flat balance objects (`{"currency": "SAR", ...}`)
/// and Moyasar's array-wrapped envelope (`{"balances": [{"currency": "SAR", ...}]}`).
///
/// # Examples
///
/// ```
/// use some_moyasar::kernel::types::{Amount, Currency};
/// use some_moyasar::resources::settlements::balance::MerchantBalance;
///
/// let json = r#"{
///     "currency": "SAR",
///     "available": 150000,
///     "pending": 25000,
///     "reserved": 5000,
///     "available_format": "1,500.00 SAR"
/// }"#;
/// let balance: MerchantBalance = serde_json::from_str(json).unwrap();
/// assert_eq!(balance.currency, Currency::SAR);
/// assert_eq!(balance.available, Amount::from_minor(150_000));
/// assert_eq!(balance.pending, Amount::from_minor(25_000));
/// assert_eq!(balance.reserved, Amount::from_minor(5_000));
/// assert_eq!(balance.available_format.as_deref(), Some("1,500.00 SAR"));
/// assert_eq!(balance.pending_format, None);
/// assert_eq!(balance.reserved_format, None);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MerchantBalance {
    /// ISO 4217 currency of the balance amounts.
    pub currency: Currency,
    /// Funds currently available for settlement, in minor currency units.
    pub available: Amount,
    /// Funds awaiting clearance before becoming available, in minor currency units.
    pub pending: Amount,
    /// Funds held in reserve, in minor currency units.
    pub reserved: Amount,
    /// Human-readable formatted available balance, if provided by the API.
    pub available_format: Option<String>,
    /// Human-readable formatted pending balance, if provided by the API.
    pub pending_format: Option<String>,
    /// Human-readable formatted reserved balance, if provided by the API.
    pub reserved_format: Option<String>,
}

#[derive(Deserialize)]
struct MerchantBalanceFields {
    currency: Currency,
    available: Amount,
    pending: Amount,
    reserved: Amount,
    #[serde(default)]
    available_format: Option<String>,
    #[serde(default)]
    pending_format: Option<String>,
    #[serde(default)]
    reserved_format: Option<String>,
}

#[derive(Deserialize)]
struct MerchantBalanceEnvelope {
    balances: Vec<MerchantBalanceFields>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum MerchantBalanceWire {
    Flat(MerchantBalanceFields),
    Envelope(MerchantBalanceEnvelope),
}

impl From<MerchantBalanceFields> for MerchantBalance {
    fn from(fields: MerchantBalanceFields) -> Self {
        Self {
            currency: fields.currency,
            available: fields.available,
            pending: fields.pending,
            reserved: fields.reserved,
            available_format: fields.available_format,
            pending_format: fields.pending_format,
            reserved_format: fields.reserved_format,
        }
    }
}

impl TryFrom<MerchantBalanceWire> for MerchantBalance {
    type Error = &'static str;

    fn try_from(wire: MerchantBalanceWire) -> Result<Self, Self::Error> {
        match wire {
            MerchantBalanceWire::Flat(fields) => Ok(Self::from(fields)),
            MerchantBalanceWire::Envelope(mut envelope) => envelope
                .balances
                .drain(..)
                .next()
                .map(Self::from)
                .ok_or("balances array must contain at least one balance entry"),
        }
    }
}

impl<'de> Deserialize<'de> for MerchantBalance {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = MerchantBalanceWire::deserialize(deserializer)?;
        Self::try_from(wire).map_err(serde::de::Error::custom)
    }
}

impl<'a, K: SecretAuthKey> SettlementsService<'a, K> {
    /// Fetches the merchant's current settlement balance via `GET /v1/settlements/balance`.
    ///
    /// Restricted at compile time to [`SecretAuthKey`] clients (`SecretKey`).
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::Api`] when the API responds with a non-success status,
    /// [`MoyasarError::ServerError`] if the response body cannot be decoded, or a
    /// transport/network [`MoyasarError`] if the HTTP request fails.
    pub async fn fetch_balance(&self) -> Result<MerchantBalance, MoyasarError> {
        let url = format!(
            "{}/v1/settlements/balance",
            self.client().config().base_url.trim_end_matches('/')
        );
        let http = self.client().http();
        let retries = self.client().config().max_network_retries;

        execute_with_retry(retries, || async {
            let response = http.get(&url).send().await?;
            parse_balance_response(response).await
        })
        .await
    }
}

async fn parse_balance_response(
    response: reqwest::Response,
) -> Result<MerchantBalance, MoyasarError> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(MoyasarError::from_api_response(status.as_u16(), &body));
    }
    serde_json::from_str::<MerchantBalance>(&body).map_err(|err| MoyasarError::ServerError {
        status: status.as_u16(),
        message: format!("failed to decode MerchantBalance response: {err}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::error::ApiErrorType;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn tc_6_1_0_06_deserializes_merchant_balance_with_optional_formats() {
        let full_json = json!({
            "currency": "SAR",
            "available": 250_000,
            "pending": 45_000,
            "reserved": 10_000,
            "available_format": "2,500.00 SAR",
            "pending_format": "450.00 SAR",
            "reserved_format": "100.00 SAR"
        });
        let full: MerchantBalance =
            serde_json::from_value(full_json).expect("full MerchantBalance deserializes");
        assert_eq!(full.currency, Currency::SAR);
        assert_eq!(full.available, Amount::from_minor(250_000));
        assert_eq!(full.pending, Amount::from_minor(45_000));
        assert_eq!(full.reserved, Amount::from_minor(10_000));
        assert_eq!(full.available_format.as_deref(), Some("2,500.00 SAR"));
        assert_eq!(full.pending_format.as_deref(), Some("450.00 SAR"));
        assert_eq!(full.reserved_format.as_deref(), Some("100.00 SAR"));

        let minimal_json = json!({
            "currency": "USD",
            "available": 10_000,
            "pending": 0,
            "reserved": 500
        });
        let minimal: MerchantBalance =
            serde_json::from_value(minimal_json).expect("minimal MerchantBalance deserializes");
        assert_eq!(minimal.currency, Currency::USD);
        assert_eq!(minimal.available, Amount::from_minor(10_000));
        assert_eq!(minimal.pending, Amount::from_minor(0));
        assert_eq!(minimal.reserved, Amount::from_minor(500));
        assert_eq!(minimal.available_format, None);
        assert_eq!(minimal.pending_format, None);
        assert_eq!(minimal.reserved_format, None);
    }

    #[test]
    fn deserializes_flat_and_balances_envelope_payloads() {
        let flat_payload = json!({
            "currency": "SAR",
            "available": 180_000,
            "pending": 20_000,
            "reserved": 5_000,
            "available_format": "1,800.00 SAR"
        });
        let envelope_payload = json!({
            "balances": [
                {
                    "currency": "SAR",
                    "available": 180_000,
                    "pending": 20_000,
                    "reserved": 5_000,
                    "available_format": "1,800.00 SAR"
                }
            ]
        });

        let from_flat: MerchantBalance =
            serde_json::from_value(flat_payload).expect("flat payload deserializes");
        let from_envelope: MerchantBalance =
            serde_json::from_value(envelope_payload).expect("balances envelope deserializes");
        assert_eq!(from_flat, from_envelope);
        assert_eq!(from_envelope.currency, Currency::SAR);
        assert_eq!(from_envelope.available, Amount::from_minor(180_000));

        let empty_envelope = json!({ "balances": [] });
        assert!(serde_json::from_value::<MerchantBalance>(empty_envelope).is_err());
    }

    #[tokio::test]
    async fn tc_6_1_0_06_fetch_balance_wiremock_success_and_error() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/settlements/balance"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "currency": "SAR",
                "available": 150_000,
                "pending": 25_000,
                "reserved": 5_000,
                "available_format": "1,500.00 SAR",
                "pending_format": "250.00 SAR",
                "reserved_format": "50.00 SAR"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let url = format!("{}/v1/settlements/balance", server.uri());
        let balance = execute_with_retry(1, || async {
            let resp = http.get(&url).send().await?;
            parse_balance_response(resp).await
        })
        .await
        .expect("fetches merchant balance");

        assert_eq!(balance.currency, Currency::SAR);
        assert_eq!(balance.available, Amount::from_minor(150_000));
        assert_eq!(balance.pending, Amount::from_minor(25_000));
        assert_eq!(balance.reserved, Amount::from_minor(5_000));
        assert_eq!(balance.available_format.as_deref(), Some("1,500.00 SAR"));

        let err_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/settlements/balance"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "type": "authentication_error",
                "message": "Invalid authentication credentials"
            })))
            .expect(1)
            .mount(&err_server)
            .await;

        let err_url = format!("{}/v1/settlements/balance", err_server.uri());
        let err = execute_with_retry(1, || async {
            let resp = http.get(&err_url).send().await?;
            parse_balance_response(resp).await
        })
        .await
        .expect_err("401 maps to ApiErrorType::AuthenticationError");

        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::AuthenticationError,
                ..
            }
        ));
    }
}
