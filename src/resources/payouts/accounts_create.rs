//! Payout account creation (`CreatePayoutAccountParams`, `PayoutAccountsService::create`).

use core::fmt;

use secrecy::{ExposeSecret, SecretString};
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::client::MoyasarClient;
use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::transport::execute_with_retry;
use crate::resources::payouts::accounts_retrieve::validate_sa_iban;
use crate::resources::payouts::models::{
    PayoutAccount, PayoutAccountProperties, PayoutAccountType,
};
use crate::resources::payouts::service::PayoutAccountsService;

const PAYOUT_ACCOUNTS_PATH: &str = "/v1/payout_accounts";

/// Request payload for registering a bank or wallet payout source account
/// (`POST /v1/payout_accounts`).
///
/// Provider `credentials` are write-only, stored in a [`SecretString`], and
/// redacted as `"[REDACTED]"` in [`fmt::Debug`] output.
///
/// # Examples
///
/// ```no_run
/// use std::collections::BTreeMap;
///
/// use secrecy::SecretString;
/// use some_moyasar::prelude::{MoyasarClient, SecretKey};
/// use some_moyasar::resources::payouts::accounts_create::CreatePayoutAccountParams;
/// use some_moyasar::resources::payouts::models::{PayoutAccountProperties, PayoutAccountType};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let key = SecretKey::new("sk_test_YOUR_SECRET_KEY")?;
/// let client = MoyasarClient::new(key)?;
///
/// let bank_props = PayoutAccountProperties {
///     iban: Some("SA4420000001234567891234".to_owned()),
///     name: Some("Acme Marketplace LLC".to_owned()),
///     bank_name: Some("Riyad Bank".to_owned()),
///     extra: BTreeMap::new(),
/// };
/// let bank_params = CreatePayoutAccountParams::new(
///     PayoutAccountType::Bank,
///     bank_props,
///     SecretString::from(r#"{"api_key":"secret_bank_cred"}"#),
/// )?;
/// let _bank_account = client.payout_accounts().create(&bank_params).await?;
///
/// let wallet_props = PayoutAccountProperties {
///     iban: None,
///     name: Some("Acme Wallet".to_owned()),
///     bank_name: None,
///     extra: BTreeMap::new(),
/// };
/// let wallet_params = CreatePayoutAccountParams::new(
///     PayoutAccountType::Wallet,
///     wallet_props,
///     SecretString::from(r#"{"wallet_token":"secret_wallet_cred"}"#),
/// )?;
/// let _wallet_account = client.payout_accounts().create(&wallet_params).await?;
/// # Ok(())
/// # }
/// ```
pub struct CreatePayoutAccountParams {
    /// Target payout account channel (`bank` or `wallet`).
    pub account_type: PayoutAccountType,
    /// Account metadata and channel properties (such as Saudi IBAN and holder name).
    pub properties: PayoutAccountProperties,
    credentials: SecretString,
}

impl CreatePayoutAccountParams {
    /// Validates and constructs a new [`CreatePayoutAccountParams`].
    ///
    /// For [`PayoutAccountType::Bank`] accounts, `properties.iban` must be present
    /// and pass [`validate_sa_iban`], and `properties.name` must be non-empty.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::InvalidIban`] if a bank IBAN fails Saudi IBAN
    /// validation, or [`MoyasarError::Api`] with [`ApiErrorType::InvalidRequestError`]
    /// if a required bank IBAN or payee name is missing or empty.
    pub fn new(
        account_type: PayoutAccountType,
        properties: PayoutAccountProperties,
        credentials: SecretString,
    ) -> Result<Self, MoyasarError> {
        if matches!(account_type, PayoutAccountType::Bank) {
            validate_bank_properties(&properties)?;
        }
        Ok(Self {
            account_type,
            properties,
            credentials,
        })
    }
}

fn validate_bank_properties(properties: &PayoutAccountProperties) -> Result<(), MoyasarError> {
    let iban = properties.iban.as_deref().ok_or_else(|| MoyasarError::Api {
        error_type: ApiErrorType::InvalidRequestError,
        message: "bank payout account requires an IBAN".to_owned(),
        errors: Vec::new(),
        response_code: None,
    })?;
    validate_sa_iban(iban)?;

    let has_name = properties
        .name
        .as_deref()
        .is_some_and(|name| !name.trim().is_empty());
    if !has_name {
        return Err(MoyasarError::Api {
            error_type: ApiErrorType::InvalidRequestError,
            message: "bank payout account requires a non-empty account holder name".to_owned(),
            errors: Vec::new(),
            response_code: None,
        });
    }
    Ok(())
}

impl fmt::Debug for CreatePayoutAccountParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreatePayoutAccountParams")
            .field("account_type", &self.account_type)
            .field("properties", &self.properties)
            .field("credentials", &"[REDACTED]")
            .finish()
    }
}

impl Serialize for CreatePayoutAccountParams {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("CreatePayoutAccountParams", 3)?;
        state.serialize_field("account_type", &self.account_type)?;
        state.serialize_field("properties", &self.properties)?;
        let s = self.credentials.expose_secret();
        let cred_val = serde_json::from_str::<serde_json::Value>(s)
            .unwrap_or_else(|_| serde_json::Value::String(s.to_owned()));
        state.serialize_field("credentials", &cred_val)?;
        state.end()
    }
}

impl<'a, K: SecretAuthKey> PayoutAccountsService<'a, K> {
    /// Registers a new bank or wallet payout source account (`POST /v1/payout_accounts`).
    ///
    /// Requires a [`SecretAuthKey`] (such as [`crate::kernel::auth::SecretKey`]);
    /// calling this with [`crate::kernel::auth::PublishableKey`] is rejected at compile time.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::Api`] on 4xx API rejections, [`MoyasarError::ServerError`]
    /// on unrecoverable 5xx responses, or [`MoyasarError::Network`] / [`MoyasarError::Timeout`]
    /// when transport retries are exhausted.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::collections::BTreeMap;
    ///
    /// use secrecy::SecretString;
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    /// use some_moyasar::resources::payouts::accounts_create::CreatePayoutAccountParams;
    /// use some_moyasar::resources::payouts::models::{PayoutAccountProperties, PayoutAccountType};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let key = SecretKey::new("sk_test_YOUR_SECRET_KEY")?;
    /// let client = MoyasarClient::new(key)?;
    /// let props = PayoutAccountProperties {
    ///     iban: Some("SA4420000001234567891234".to_owned()),
    ///     name: Some("Acme Marketplace LLC".to_owned()),
    ///     bank_name: None,
    ///     extra: BTreeMap::new(),
    /// };
    /// let params = CreatePayoutAccountParams::new(
    ///     PayoutAccountType::Bank,
    ///     props,
    ///     SecretString::from(r#"{"client_id":"cid"}"#),
    /// )?;
    /// let account = client.payout_accounts().create(&params).await?;
    /// println!("Created payout account: {:?}", account.id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create(
        &self,
        params: &CreatePayoutAccountParams,
    ) -> Result<PayoutAccount, MoyasarError> {
        let base_url = &self.client().config().base_url;
        self.create_at(base_url, params).await
    }

    /// Registers a new payout source account against an explicit `base_url`
    /// (`POST {base_url}/v1/payout_accounts`).
    ///
    /// Primarily useful for integration testing with mock servers.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::Api`] on 4xx API rejections, [`MoyasarError::ServerError`]
    /// on unrecoverable 5xx responses, or [`MoyasarError::Network`] / [`MoyasarError::Timeout`]
    /// when transport retries are exhausted.
    pub async fn create_at(
        &self,
        base_url: &str,
        params: &CreatePayoutAccountParams,
    ) -> Result<PayoutAccount, MoyasarError> {
        let client = self.client();
        let url = format!("{}{}", base_url.trim_end_matches('/'), PAYOUT_ACCOUNTS_PATH);
        execute_with_retry(client.config().max_network_retries, || {
            send_create_request(client, &url, params)
        })
        .await
    }
}

async fn send_create_request<K: SecretAuthKey>(
    client: &MoyasarClient<K>,
    url: &str,
    params: &CreatePayoutAccountParams,
) -> Result<PayoutAccount, MoyasarError> {
    let response = client
        .http()
        .post(url)
        .json(params)
        .send()
        .await
        .map_err(map_reqwest_error)?;
    let status = response.status();
    let text = response.text().await.map_err(map_reqwest_error)?;
    if !status.is_success() {
        return Err(MoyasarError::from_api_response(status.as_u16(), &text));
    }
    serde_json::from_str::<PayoutAccount>(&text)
        .map_err(|_| MoyasarError::from_api_response(status.as_u16(), &text))
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
    use std::collections::BTreeMap;

    use serde_json::json;
    use wiremock::matchers::{body_json, body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::kernel::auth::SecretKey;

    fn sample_bank_properties() -> PayoutAccountProperties {
        PayoutAccountProperties {
            iban: Some("SA4420000001234567891234".to_owned()),
            name: Some("Acme Marketplace LLC".to_owned()),
            bank_name: Some("Riyad Bank".to_owned()),
            extra: BTreeMap::new(),
        }
    }

    #[tokio::test]
    async fn tc_5_1_0_01_creates_bank_payout_account_with_iban() {
        let server = MockServer::start().await;
        let expected_body = json!({
            "account_type": "bank",
            "properties": {
                "iban": "SA4420000001234567891234",
                "name": "Acme Marketplace LLC",
                "bank_name": "Riyad Bank"
            },
            "credentials": {
                "api_key": "bank_secret_value"
            }
        });
        let response_json = json!({
            "id": "11111111-1111-4111-8111-111111111111",
            "account_type": "bank",
            "currency": "SAR",
            "properties": {
                "iban": "SA4420000001234567891234",
                "name": "Acme Marketplace LLC",
                "bank_name": "Riyad Bank"
            },
            "created_at": "2026-11-14T00:00:00Z"
        });

        Mock::given(method("POST"))
            .and(path("/v1/payout_accounts"))
            .and(body_json(&expected_body))
            .respond_with(ResponseTemplate::new(201).set_body_json(&response_json))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_payout_bank").expect("valid secret key");
        let client = MoyasarClient::new(key).expect("client builds");
        let params = CreatePayoutAccountParams::new(
            PayoutAccountType::Bank,
            sample_bank_properties(),
            SecretString::from(r#"{"api_key":"bank_secret_value"}"#),
        )
        .expect("valid bank params");

        let account = client
            .payout_accounts()
            .create_at(&server.uri(), &params)
            .await
            .expect("creates bank payout account");

        assert_eq!(account.account_type, PayoutAccountType::Bank);
        assert_eq!(
            account.properties.iban.as_deref(),
            Some("SA4420000001234567891234")
        );
    }

    #[tokio::test]
    async fn tc_5_1_0_02_creates_wallet_payout_account() {
        let server = MockServer::start().await;
        let expected_partial = json!({
            "account_type": "wallet",
            "credentials": "raw_wallet_token"
        });
        let response_json = json!({
            "id": "22222222-2222-4222-8222-222222222222",
            "account_type": "wallet",
            "currency": "SAR",
            "properties": {
                "name": "Acme Wallet"
            },
            "created_at": "2026-11-14T00:00:00Z"
        });

        Mock::given(method("POST"))
            .and(path("/v1/payout_accounts"))
            .and(body_partial_json(&expected_partial))
            .respond_with(ResponseTemplate::new(201).set_body_json(&response_json))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_payout_wallet").expect("valid secret key");
        let client = MoyasarClient::new(key).expect("client builds");
        let wallet_props = PayoutAccountProperties {
            iban: None,
            name: Some("Acme Wallet".to_owned()),
            bank_name: None,
            extra: BTreeMap::new(),
        };
        let params = CreatePayoutAccountParams::new(
            PayoutAccountType::Wallet,
            wallet_props,
            SecretString::from("raw_wallet_token"),
        )
        .expect("valid wallet params");

        let account = client
            .payout_accounts()
            .create_at(&server.uri(), &params)
            .await
            .expect("creates wallet payout account");

        assert_eq!(account.account_type, PayoutAccountType::Wallet);
        assert_eq!(account.properties.name.as_deref(), Some("Acme Wallet"));
    }

    #[test]
    fn tc_5_1_0_05_redacts_credentials_in_debug_output() {
        let secret_marker = "super_sensitive_provider_credential_999";
        let params = CreatePayoutAccountParams::new(
            PayoutAccountType::Bank,
            sample_bank_properties(),
            SecretString::from(format!(r#"{{"token":"{secret_marker}"}}"#)),
        )
        .expect("valid bank params");

        let debug_output = format!("{params:?}");
        assert!(debug_output.contains("[REDACTED]"));
        assert!(!debug_output.contains(secret_marker));
    }

    #[test]
    fn rejects_bank_account_with_missing_name_or_invalid_iban() {
        let missing_name = PayoutAccountProperties {
            iban: Some("SA4420000001234567891234".to_owned()),
            name: Some("   ".to_owned()),
            bank_name: None,
            extra: BTreeMap::new(),
        };
        let err = CreatePayoutAccountParams::new(
            PayoutAccountType::Bank,
            missing_name,
            SecretString::from("cred"),
        )
        .expect_err("empty bank payee name must fail");
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            }
        ));

        let invalid_iban = PayoutAccountProperties {
            iban: Some("INVALID_IBAN".to_owned()),
            name: Some("Acme LLC".to_owned()),
            bank_name: None,
            extra: BTreeMap::new(),
        };
        let err = CreatePayoutAccountParams::new(
            PayoutAccountType::Bank,
            invalid_iban,
            SecretString::from("cred"),
        )
        .expect_err("invalid SA IBAN must fail");
        assert!(matches!(err, MoyasarError::InvalidIban));
    }
}
