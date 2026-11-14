//! Payout listing and retrieval endpoints (`GET /v1/payouts`, `GET /v1/payouts/{id}`) (`US-5.2.0` Unit 6).

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::pagination::PaginatedList;
use crate::kernel::transport::execute_with_retry;
use crate::resources::payouts::payout_model::{Payout, PayoutId};
use crate::resources::payouts::service::PayoutsService;

const PAYOUTS_PATH: &str = "/v1/payouts";

/// Query parameters for listing payout disbursements (`GET /v1/payouts`).
///
/// Supports 1-based `page` selection and `per`-page size limits. Unset fields
/// are omitted during query serialization.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::payouts::payouts_list::ListPayoutsParams;
///
/// let params = ListPayoutsParams::new().page(2).per(25);
/// assert_eq!(params.page, Some(2));
/// assert_eq!(params.per, Some(25));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ListPayoutsParams {
    /// Optional 1-based page number to retrieve.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Optional maximum number of payouts to return per page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per: Option<u32>,
}

impl ListPayoutsParams {
    /// Creates an empty [`ListPayoutsParams`] with no pagination parameters set.
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

impl<'a, K: SecretAuthKey> PayoutsService<'a, K> {
    /// Lists payout disbursements with optional pagination (`GET /v1/payouts`).
    ///
    /// Requires a [`SecretAuthKey`] (e.g., [`SecretKey`](crate::kernel::auth::SecretKey)).
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError`] on network failure, timeout, or non-2xx API
    /// response.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    /// use some_moyasar::resources::payouts::payouts_list::ListPayoutsParams;
    ///
    /// # async fn example() -> Result<(), some_moyasar::prelude::MoyasarError> {
    /// let client = MoyasarClient::new(SecretKey::new("sk_test_example")?)?;
    /// let params = ListPayoutsParams::new().page(1).per(20);
    /// let page = client.payouts().list(&params).await?;
    /// println!("Total payouts: {}", page.meta.total_count);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list(
        &self,
        params: &ListPayoutsParams,
    ) -> Result<PaginatedList<Payout>, MoyasarError> {
        self.list_at(self.client().config().base_url.as_str(), params)
            .await
    }

    /// Lists payout disbursements against an explicit `base_url` (`GET {base_url}/v1/payouts`).
    ///
    /// Useful for targeting a local mock server in integration tests while
    /// preserving the client's default headers, authentication, and retry policy.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError`] on network failure, timeout, or non-2xx API
    /// response.
    pub async fn list_at(
        &self,
        base_url: &str,
        params: &ListPayoutsParams,
    ) -> Result<PaginatedList<Payout>, MoyasarError> {
        let base = base_url.trim_end_matches('/');
        let url = format!("{base}{PAYOUTS_PATH}");
        let http = self.client().http();
        let retries = self.client().config().max_network_retries;

        execute_with_retry(retries, || async {
            let response = http
                .get(&url)
                .query(params)
                .send()
                .await
                .map_err(map_reqwest_error)?;
            decode_response(response).await
        })
        .await
    }

    /// Retrieves a single payout disbursement by its [`PayoutId`] (`GET /v1/payouts/{id}`).
    ///
    /// Requires a [`SecretAuthKey`] (e.g., [`SecretKey`](crate::kernel::auth::SecretKey)).
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError`] on network failure, timeout, or non-2xx API
    /// response (such as `RecordNotFound` for an unknown payout ID).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    /// use some_moyasar::resources::payouts::payout_model::PayoutId;
    ///
    /// # async fn example() -> Result<(), some_moyasar::prelude::MoyasarError> {
    /// let client = MoyasarClient::new(SecretKey::new("sk_test_example")?)?;
    /// let id = PayoutId::new("pout_example_123")?;
    /// let payout = client.payouts().retrieve(&id).await?;
    /// println!("Payout status: {:?}", payout.status);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn retrieve(&self, id: &PayoutId) -> Result<Payout, MoyasarError> {
        self.retrieve_at(self.client().config().base_url.as_str(), id)
            .await
    }

    /// Retrieves a single payout disbursement against an explicit `base_url`
    /// (`GET {base_url}/v1/payouts/{id}`).
    ///
    /// Useful for targeting a local mock server in integration tests while
    /// preserving the client's default headers, authentication, and retry policy.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError`] on network failure, timeout, or non-2xx API
    /// response (such as `RecordNotFound` for an unknown payout ID).
    pub async fn retrieve_at(&self, base_url: &str, id: &PayoutId) -> Result<Payout, MoyasarError> {
        let base = base_url.trim_end_matches('/');
        let url = format!("{base}{PAYOUTS_PATH}/{}", id.as_str());
        let http = self.client().http();
        let retries = self.client().config().max_network_retries;

        execute_with_retry(retries, || async {
            let response = http.get(&url).send().await.map_err(map_reqwest_error)?;
            decode_response(response).await
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

async fn decode_response<T: DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, MoyasarError> {
    let status = response.status();
    let body = response.text().await.map_err(map_reqwest_error)?;
    if !status.is_success() {
        return Err(MoyasarError::from_api_response(status.as_u16(), &body));
    }
    serde_json::from_str::<T>(&body).map_err(|err| MoyasarError::ServerError {
        status: status.as_u16(),
        message: format!("failed to decode payout response: {err}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::client::MoyasarClient;
    use crate::kernel::error::ApiErrorType;
    use crate::kernel::transport::{API_VERSION, MOYASAR_VERSION_HEADER};
    use crate::resources::payouts::payout_model::{PayoutChannel, PayoutStatus};
    use serde_json::json;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn list_payouts_omits_unset_query_params() {
        let server = MockServer::start().await;
        let key = SecretKey::new("sk_test_unit6_empty").expect("valid secret key");
        let client = MoyasarClient::new(key).expect("valid client");

        Mock::given(method("GET"))
            .and(path("/v1/payouts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "payouts": [],
                "meta": {
                    "current_page": 1,
                    "next_page": null,
                    "prev_page": null,
                    "total_pages": 0,
                    "total_count": 0
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let page = client
            .payouts()
            .list_at(&server.uri(), &ListPayoutsParams::default())
            .await
            .expect("empty list succeeds");
        assert!(page.items.is_empty());

        let requests = server.received_requests().await.expect("requests recorded");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url.query(), None);
    }

    #[tokio::test]
    async fn tc_5_2_0_06_paginated_list_single_retrieve_and_404_record_not_found() {
        let server = MockServer::start().await;
        let key = SecretKey::new("sk_test_unit6_payouts").expect("valid secret key");
        let client = MoyasarClient::new(key).expect("valid client");
        let service = client.payouts();

        Mock::given(method("GET"))
            .and(path("/v1/payouts"))
            .and(query_param("page", "1"))
            .and(query_param("per", "10"))
            .and(header(MOYASAR_VERSION_HEADER, API_VERSION))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "payouts": [{
                    "id": "pout_01J9LIST",
                    "channel": "sarie",
                    "status": "paid",
                    "amount": 50000,
                    "message": "Vendor settlement",
                    "failure_reason": null
                }],
                "meta": {
                    "current_page": 1,
                    "next_page": 2,
                    "prev_page": null,
                    "total_pages": 2,
                    "total_count": 15
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let params = ListPayoutsParams::new().page(1).per(10);
        let page = service
            .list_at(&server.uri(), &params)
            .await
            .expect("list_at succeeds");

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id.as_str(), "pout_01J9LIST");
        assert_eq!(page.items[0].channel, PayoutChannel::Sarie);
        assert_eq!(page.items[0].status, PayoutStatus::Paid);
        assert_eq!(page.items[0].amount, 50_000);
        assert_eq!(page.items[0].message.as_deref(), Some("Vendor settlement"));
        assert_eq!(page.items[0].failure_reason, None);
        assert_eq!(page.meta.current_page, 1);
        assert_eq!(page.meta.next_page, Some(2));
        assert_eq!(page.meta.prev_page, None);
        assert_eq!(page.meta.total_pages, 2);
        assert_eq!(page.meta.total_count, 15);

        let payout_id = PayoutId::new("pout_01J9RETR").expect("valid payout id");
        Mock::given(method("GET"))
            .and(path("/v1/payouts/pout_01J9RETR"))
            .and(header(MOYASAR_VERSION_HEADER, API_VERSION))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "pout_01J9RETR",
                "channel": "ips",
                "status": "queued",
                "amount": 12500,
                "message": null,
                "failure_reason": null
            })))
            .expect(1)
            .mount(&server)
            .await;

        let payout = service
            .retrieve_at(&server.uri(), &payout_id)
            .await
            .expect("retrieve_at succeeds");
        assert_eq!(payout.id, payout_id);
        assert_eq!(payout.channel, PayoutChannel::Ips);
        assert_eq!(payout.status, PayoutStatus::Queued);
        assert_eq!(payout.amount, 12_500);

        let missing_id = PayoutId::new("pout_missing").expect("valid payout id");
        Mock::given(method("GET"))
            .and(path("/v1/payouts/pout_missing"))
            .and(header(MOYASAR_VERSION_HEADER, API_VERSION))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "type": "record_not_found",
                "message": "Payout not found"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let err = service
            .retrieve_at(&server.uri(), &missing_id)
            .await
            .expect_err("404 must map to RecordNotFound");
        assert!(!err.is_retryable());
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::RecordNotFound,
                ..
            }
        ));
    }
}
