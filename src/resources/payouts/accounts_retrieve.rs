//! Payout account retrieval and Saudi IBAN validation (`US-5.1.0` Unit 4).

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::transport::execute_with_retry;
use crate::resources::payouts::models::{PayoutAccount, PayoutAccountId};
use crate::resources::payouts::service::PayoutAccountsService;

const PAYOUT_ACCOUNTS_PATH: &str = "/v1/payout_accounts";

/// Validates that `iban` is a syntactically valid Saudi IBAN (`SA` + 22 ASCII digits).
///
/// Enforces SAMA outbound-transfer payee identification format requirements (`TC-5.1.0-LAW`).
///
/// # Errors
///
/// Returns [`MoyasarError::InvalidIban`] if `iban` does not start with `"SA"`,
/// is not exactly 24 bytes long, or contains any non-ASCII-digit character after `"SA"`.
///
/// # Examples
///
/// ```
/// use some_moyasar::kernel::error::MoyasarError;
/// use some_moyasar::resources::payouts::accounts_retrieve::validate_sa_iban;
///
/// assert!(validate_sa_iban("SA0380000000608010167519").is_ok());
/// assert!(matches!(
///     validate_sa_iban("SA1234"),
///     Err(MoyasarError::InvalidIban)
/// ));
/// assert!(matches!(
///     validate_sa_iban("AE070331234567890123456"),
///     Err(MoyasarError::InvalidIban)
/// ));
/// ```
pub fn validate_sa_iban(iban: &str) -> Result<(), MoyasarError> {
    if iban.starts_with("SA")
        && iban.len() == 24
        && iban[2..].bytes().all(|b| b.is_ascii_digit())
    {
        Ok(())
    } else {
        Err(MoyasarError::InvalidIban)
    }
}

impl<'a, K: SecretAuthKey> PayoutAccountsService<'a, K> {
    /// Retrieves a single payout source account by its [`PayoutAccountId`]
    /// (`GET /v1/payout_accounts/{id}`).
    ///
    /// Restricted at compile time to [`SecretAuthKey`] clients (`SecretKey`).
    /// Attempting to call `payout_accounts().retrieve()` on a
    /// [`PublishableKey`](crate::kernel::auth::PublishableKey) client fails to compile:
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::payouts::models::PayoutAccountId;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let key = PublishableKey::new("pk_test_YOUR_KEY")?;
    /// let client = MoyasarClient::new(key)?;
    /// let id = PayoutAccountId::new("pa_01J9XYZ")?;
    /// let _account = client.payout_accounts().retrieve(&id).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::Api`] with
    /// [`ApiErrorType::RecordNotFound`](crate::kernel::error::ApiErrorType::RecordNotFound)
    /// when the account does not exist (HTTP 404), or a network/server error on failure.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    /// use some_moyasar::resources::payouts::models::PayoutAccountId;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let key = SecretKey::new("sk_test_YOUR_SECRET_KEY")?;
    /// let client = MoyasarClient::new(key)?;
    /// let id = PayoutAccountId::new("pa_01J9XYZ")?;
    /// let account = client.payout_accounts().retrieve(&id).await?;
    /// assert_eq!(account.id, id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn retrieve(&self, id: &PayoutAccountId) -> Result<PayoutAccount, MoyasarError> {
        self.retrieve_at(&self.client().config().base_url, id).await
    }

    /// Retrieves a single payout source account by its [`PayoutAccountId`] against a
    /// custom `base_url` (`GET {base_url}/v1/payout_accounts/{id}`).
    ///
    /// Uses [`execute_with_retry`] with the client's configured `max_network_retries`.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::Api`] with
    /// [`ApiErrorType::RecordNotFound`](crate::kernel::error::ApiErrorType::RecordNotFound)
    /// when the account does not exist (HTTP 404), or a network/server error on failure.
    pub async fn retrieve_at(
        &self,
        base_url: &str,
        id: &PayoutAccountId,
    ) -> Result<PayoutAccount, MoyasarError> {
        let url = format!(
            "{}{PAYOUT_ACCOUNTS_PATH}/{}",
            base_url.trim_end_matches('/'),
            id.as_str()
        );
        let http = self.client().http();
        let retries = self.client().config().max_network_retries;

        execute_with_retry(retries, || async {
            let response = http.get(&url).send().await.map_err(map_reqwest_error)?;
            parse_retrieve_response(response).await
        })
        .await
    }
}

fn map_reqwest_error(err: reqwest::Error) -> MoyasarError {
    if err.is_timeout() {
        MoyasarError::Timeout
    } else {
        MoyasarError::Network(err)
    }
}

async fn parse_retrieve_response(
    response: reqwest::Response,
) -> Result<PayoutAccount, MoyasarError> {
    let status = response.status();
    let body = response.text().await.map_err(map_reqwest_error)?;
    if !status.is_success() {
        return Err(MoyasarError::from_api_response(status.as_u16(), &body));
    }
    serde_json::from_str::<PayoutAccount>(&body).map_err(|err| MoyasarError::ServerError {
        status: status.as_u16(),
        message: format!("failed to decode PayoutAccount response: {err}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::client::MoyasarClient;
    use crate::kernel::error::ApiErrorType;
    use crate::kernel::types::Currency;
    use crate::resources::payouts::models::PayoutAccountType;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn tc_5_1_0_04_retrieve_returns_payout_account_and_maps_404_to_record_not_found() {
        let server = MockServer::start().await;
        let id = PayoutAccountId::new("pa_01J9RETRIEVE").expect("valid payout account id");

        Mock::given(method("GET"))
            .and(path("/v1/payout_accounts/pa_01J9RETRIEVE"))
            .and(header("moyasar-version", "2024-01-01"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "pa_01J9RETRIEVE",
                "account_type": "bank",
                "currency": "SAR",
                "properties": {
                    "iban": "SA0380000000608010167519",
                    "name": "Acme Trading LLC",
                    "bank_name": "Al Rajhi Bank"
                },
                "created_at": "2026-11-14T00:00:00Z"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_payout_retrieve").expect("valid secret key");
        let client = MoyasarClient::builder(key)
            .max_network_retries(1)
            .build()
            .expect("client builds");

        let account = client
            .payout_accounts()
            .retrieve_at(&server.uri(), &id)
            .await
            .expect("retrieves payout account");
        assert_eq!(account.id, id);
        assert_eq!(account.account_type, PayoutAccountType::Bank);
        assert_eq!(account.currency, Currency::SAR);
        assert_eq!(
            account.properties.iban.as_deref(),
            Some("SA0380000000608010167519")
        );
        assert_eq!(
            account.properties.name.as_deref(),
            Some("Acme Trading LLC")
        );

        let missing_id = PayoutAccountId::new("pa_missing_404").expect("valid payout account id");
        Mock::given(method("GET"))
            .and(path("/v1/payout_accounts/pa_missing_404"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "type": "record_not_found",
                "message": "Payout account not found"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let err = client
            .payout_accounts()
            .retrieve_at(&server.uri(), &missing_id)
            .await
            .expect_err("404 must map to RecordNotFound");

        assert!(!err.is_retryable());
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::RecordNotFound,
                ref message,
                ..
            } if message == "Payout account not found"
        ));
    }

    #[test]
    fn tc_5_1_0_law_validates_saudi_iban_format() {
        for valid in [
            "SA0380000000608010167519",
            "SA4420000001234567891234",
            "SA0000000000000000000000",
        ] {
            assert!(
                validate_sa_iban(valid).is_ok(),
                "expected valid Saudi IBAN for {valid:?}"
            );
        }

        for invalid in [
            "",
            "SA",
            "SA038000000060801016751",
            "SA03800000006080101675190",
            "sa0380000000608010167519",
            "AE070331234567890123456",
            "SA038000000060801016751A",
            "SA03 8000000060801016751",
            "SA03-8000000060801016751",
            "SA038000000060801016751é",
        ] {
            assert!(
                matches!(validate_sa_iban(invalid), Err(MoyasarError::InvalidIban)),
                "expected InvalidIban for {invalid:?}"
            );
        }
    }
}
