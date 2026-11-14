//! Payout account listing endpoint (`GET /v1/payout_accounts`) for `PayoutAccountsService` (`US-5.1.0` Unit 3).

use serde::Serialize;

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::pagination::PaginatedList;
use crate::kernel::transport::execute_with_retry;
use crate::resources::payouts::models::PayoutAccount;
use crate::resources::payouts::service::PayoutAccountsService;

const PAYOUT_ACCOUNTS_PATH: &str = "/v1/payout_accounts";

/// Query pagination parameters for `GET /v1/payout_accounts`.
///
/// # Examples
///
/// ```no_run
/// use some_moyasar::prelude::{MoyasarClient, SecretKey};
/// use some_moyasar::resources::payouts::accounts_list::ListPayoutAccountsParams;
///
/// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
/// let key = SecretKey::new("sk_test_YOUR_SECRET_KEY")?;
/// let client = MoyasarClient::new(key)?;
/// let params = ListPayoutAccountsParams::new().page(1).per(25);
/// let page = client.payout_accounts().list(&params).await?;
/// println!("Found {} payout accounts", page.items.len());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ListPayoutAccountsParams {
    /// 1-based page number to fetch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Number of payout accounts to return per page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per: Option<u32>,
}

impl ListPayoutAccountsParams {
    /// Creates an empty [`ListPayoutAccountsParams`] with no pagination overrides set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the 1-based `page` query parameter.
    #[must_use]
    pub const fn page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }

    /// Sets the `per` (items per page) query parameter.
    #[must_use]
    pub const fn per(mut self, per: u32) -> Self {
        self.per = Some(per);
        self
    }
}

impl<'a, K: SecretAuthKey> PayoutAccountsService<'a, K> {
    /// Lists payout accounts matching [`ListPayoutAccountsParams`] via `GET /v1/payout_accounts`.
    ///
    /// Restricted at compile time to [`SecretAuthKey`] clients (`SecretKey`).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    /// use some_moyasar::resources::payouts::accounts_list::ListPayoutAccountsParams;
    ///
    /// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
    /// let key = SecretKey::new("sk_test_YOUR_SECRET_KEY")?;
    /// let client = MoyasarClient::new(key)?;
    /// let page = client
    ///     .payout_accounts()
    ///     .list(&ListPayoutAccountsParams::default())
    ///     .await?;
    /// println!("Total payout accounts: {}", page.meta.total_count);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list(
        &self,
        params: &ListPayoutAccountsParams,
    ) -> Result<PaginatedList<PayoutAccount>, MoyasarError> {
        self.list_at(&self.client().config().base_url, params).await
    }

    /// Lists payout accounts against an explicit `base_url` via `GET {base_url}/v1/payout_accounts`.
    ///
    /// Primarily useful for testing against local mock servers or custom API base URLs.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    /// use some_moyasar::resources::payouts::accounts_list::ListPayoutAccountsParams;
    ///
    /// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
    /// let key = SecretKey::new("sk_test_YOUR_SECRET_KEY")?;
    /// let client = MoyasarClient::new(key)?;
    /// let page = client
    ///     .payout_accounts()
    ///     .list_at("https://api.moyasar.com", &ListPayoutAccountsParams::new().page(1))
    ///     .await?;
    /// println!("Fetched {} payout accounts", page.items.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_at(
        &self,
        base_url: &str,
        params: &ListPayoutAccountsParams,
    ) -> Result<PaginatedList<PayoutAccount>, MoyasarError> {
        let base = base_url.trim_end_matches('/');
        let url = format!("{base}{PAYOUT_ACCOUNTS_PATH}");
        let http = self.client().http();
        let retries = self.client().config().max_network_retries;

        execute_with_retry(retries, || async {
            let response = http
                .get(&url)
                .query(params)
                .send()
                .await
                .map_err(MoyasarError::from)?;
            parse_list_response(response).await
        })
        .await
    }
}

async fn parse_list_response(
    response: reqwest::Response,
) -> Result<PaginatedList<PayoutAccount>, MoyasarError> {
    let status = response.status();
    let body = response.text().await.map_err(MoyasarError::from)?;
    if !status.is_success() {
        return Err(MoyasarError::from_api_response(status.as_u16(), &body));
    }
    serde_json::from_str::<PaginatedList<PayoutAccount>>(&body).map_err(|err| {
        MoyasarError::ServerError {
            status: status.as_u16(),
            message: format!("failed to decode PaginatedList<PayoutAccount>: {err}"),
        }
    })
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::client::MoyasarClient;
    use crate::kernel::types::Currency;
    use crate::resources::payouts::models::PayoutAccountType;
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn tc_5_1_0_03_lists_paginated_payout_accounts_with_query_params() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/payout_accounts"))
            .and(query_param("page", "2"))
            .and(query_param("per", "10"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "payout_accounts": [
                    {
                        "id": "pa_01J9BANK01",
                        "account_type": "bank",
                        "currency": "SAR",
                        "properties": {
                            "iban": "SA0380000000608010167519",
                            "name": "Acme Trading LLC",
                            "bank_name": "Al Rajhi Bank"
                        },
                        "created_at": "2026-11-14T00:00:00Z"
                    },
                    {
                        "id": "pa_01J9WALLET02",
                        "account_type": "wallet",
                        "currency": "SAR",
                        "properties": {
                            "name": "Merchant Wallet"
                        },
                        "created_at": "2026-11-14T01:00:00Z"
                    }
                ],
                "meta": {
                    "current_page": 2,
                    "next_page": 3,
                    "prev_page": 1,
                    "total_pages": 4,
                    "total_count": 32
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_payout_accounts_list").expect("valid secret key");
        let client = MoyasarClient::new(key).expect("valid client");
        let params = ListPayoutAccountsParams::new().page(2).per(10);

        let page = client
            .payout_accounts()
            .list_at(&server.uri(), &params)
            .await
            .expect("lists payout accounts");

        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].id.as_str(), "pa_01J9BANK01");
        assert_eq!(page.items[0].account_type, PayoutAccountType::Bank);
        assert_eq!(page.items[0].currency, Currency::SAR);
        assert_eq!(page.items[1].id.as_str(), "pa_01J9WALLET02");
        assert_eq!(page.items[1].account_type, PayoutAccountType::Wallet);
        assert_eq!(page.meta.current_page, 2);
        assert_eq!(page.meta.next_page, Some(3));
        assert_eq!(page.meta.prev_page, Some(1));
        assert_eq!(page.meta.total_pages, 4);
        assert_eq!(page.meta.total_count, 32);
    }

    #[tokio::test]
    async fn list_at_handles_default_params_and_empty_page() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/payout_accounts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "payout_accounts": [],
                "meta": {
                    "current_page": 1,
                    "next_page": null,
                    "prev_page": null,
                    "total_pages": 1,
                    "total_count": 0
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_payout_accounts_empty").expect("valid secret key");
        let client = MoyasarClient::new(key).expect("valid client");

        let page = client
            .payout_accounts()
            .list_at(&server.uri(), &ListPayoutAccountsParams::default())
            .await
            .expect("lists empty payout accounts");

        assert!(page.items.is_empty());
        assert_eq!(page.meta.current_page, 1);
        assert_eq!(page.meta.next_page, None);
        assert_eq!(page.meta.total_count, 0);
    }

    #[tokio::test]
    async fn list_at_propagates_api_error_response() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/payout_accounts"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "type": "authentication_error",
                "message": "Invalid authentication credentials"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_payout_accounts_err").expect("valid secret key");
        let client = MoyasarClient::new(key).expect("valid client");

        let err = client
            .payout_accounts()
            .list_at(&server.uri(), &ListPayoutAccountsParams::default())
            .await
            .expect_err("expected 401 api error");

        assert!(!err.is_retryable());
    }
}
