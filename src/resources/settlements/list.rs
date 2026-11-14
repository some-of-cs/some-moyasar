//! Settlement listing (`ListSettlementsParams`, `SettlementsService::list`).

use chrono::{DateTime, SecondsFormat, Utc};
use serde::de::DeserializeOwned;

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::pagination::PaginatedList;
use crate::kernel::transport::execute_with_retry;
use crate::resources::settlements::models::{Settlement, SettlementId};
use crate::resources::settlements::service::SettlementsService;

const SETTLEMENTS_PATH: &str = "/v1/settlements";

/// Query filter and pagination parameters for listing settlements (`GET /v1/settlements`).
///
/// Supports pagination (`page`), exact `id` filtering, and `created[gt]` / `created[lt]`
/// UTC timestamp bounds formatted as RFC 3339 seconds (`YYYY-MM-DDTHH:MM:SSZ`).
///
/// # Examples
///
/// ```
/// use chrono::{DateTime, Utc};
/// use some_moyasar::resources::settlements::list::ListSettlementsParams;
///
/// let since: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
/// let until: DateTime<Utc> = "2026-02-01T00:00:00Z".parse().unwrap();
/// let params = ListSettlementsParams::new()
///     .page(2)
///     .created_gt(since)
///     .created_lt(until);
///
/// assert_eq!(
///     params.to_query_pairs(),
///     vec![
///         ("page".to_owned(), "2".to_owned()),
///         ("created[gt]".to_owned(), "2026-01-01T00:00:00Z".to_owned()),
///         ("created[lt]".to_owned(), "2026-02-01T00:00:00Z".to_owned()),
///     ]
/// );
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListSettlementsParams {
    /// 1-based page number to fetch.
    pub page: Option<u32>,
    /// Filter by an exact [`SettlementId`].
    pub id: Option<SettlementId>,
    /// Filter settlements created strictly after this UTC timestamp (`created[gt]`).
    pub created_gt: Option<DateTime<Utc>>,
    /// Filter settlements created strictly before this UTC timestamp (`created[lt]`).
    pub created_lt: Option<DateTime<Utc>>,
}

impl ListSettlementsParams {
    /// Creates an empty [`ListSettlementsParams`] with no filters set.
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

    /// Filters by a specific [`SettlementId`] (`id`).
    #[must_use]
    pub fn id(mut self, id: SettlementId) -> Self {
        self.id = Some(id);
        self
    }

    /// Filters for settlements created strictly after `ts` (`created[gt]`).
    #[must_use]
    pub const fn created_gt(mut self, ts: DateTime<Utc>) -> Self {
        self.created_gt = Some(ts);
        self
    }

    /// Filters for settlements created strictly before `ts` (`created[lt]`).
    #[must_use]
    pub const fn created_lt(mut self, ts: DateTime<Utc>) -> Self {
        self.created_lt = Some(ts);
        self
    }

    /// Converts all configured filters into ordered `(key, value)` query pairs.
    #[must_use]
    pub fn to_query_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        if let Some(page) = self.page {
            pairs.push(("page".to_owned(), page.to_string()));
        }
        if let Some(id) = &self.id {
            pairs.push(("id".to_owned(), id.to_string()));
        }
        if let Some(created_gt) = self.created_gt {
            pairs.push((
                "created[gt]".to_owned(),
                created_gt.to_rfc3339_opts(SecondsFormat::Secs, true),
            ));
        }
        if let Some(created_lt) = self.created_lt {
            pairs.push((
                "created[lt]".to_owned(),
                created_lt.to_rfc3339_opts(SecondsFormat::Secs, true),
            ));
        }
        pairs
    }
}

impl<'a, K: SecretAuthKey> SettlementsService<'a, K> {
    /// Lists settlements matching [`ListSettlementsParams`] via `GET /v1/settlements`.
    ///
    /// Restricted at compile time to [`SecretAuthKey`] clients (e.g., [`SecretKey`](crate::kernel::auth::SecretKey)).
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
    /// use some_moyasar::resources::settlements::list::ListSettlementsParams;
    ///
    /// # async fn example() -> Result<(), some_moyasar::prelude::MoyasarError> {
    /// let client = MoyasarClient::new(SecretKey::new("sk_test_example")?)?;
    /// let params = ListSettlementsParams::new().page(1);
    /// let page = client.settlements().list(&params).await?;
    /// println!("Total settlements: {}", page.meta.total_count);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// A [`PublishableKey`](crate::kernel::auth::PublishableKey) cannot call
    /// `list` (`SecretAuthKey` is required):
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::settlements::list::ListSettlementsParams;
    ///
    /// # async fn example() {
    /// let pk = PublishableKey::new("pk_test_example").unwrap();
    /// let client = MoyasarClient::new(pk).unwrap();
    /// let _ = client.settlements().list(&ListSettlementsParams::new()).await;
    /// # }
    /// ```
    pub async fn list(
        &self,
        params: &ListSettlementsParams,
    ) -> Result<PaginatedList<Settlement>, MoyasarError> {
        let base = self.client().config().base_url.trim_end_matches('/');
        let url = format!("{base}{SETTLEMENTS_PATH}");
        let query = params.to_query_pairs();
        send_get(
            self.client().http(),
            &url,
            self.client().config().max_network_retries,
            &query,
        )
        .await
    }
}

async fn send_get<T: DeserializeOwned>(
    http: &reqwest::Client,
    url: &str,
    max_retries: u32,
    query: &[(String, String)],
) -> Result<T, MoyasarError> {
    execute_with_retry(max_retries, || async {
        let response = http
            .get(url)
            .query(query)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        decode_response(response).await
    })
    .await
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
    if status.is_success() {
        return response.json::<T>().await.map_err(map_reqwest_error);
    }
    let body = response.text().await.map_err(map_reqwest_error)?;
    Err(MoyasarError::from_api_response(status.as_u16(), &body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::error::ApiErrorType;
    use crate::kernel::transport::{API_VERSION, MOYASAR_VERSION_HEADER, build_default_headers};
    use serde_json::{Value, json};
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_http_client() -> reqwest::Client {
        let key = SecretKey::new("sk_test_settlement_unit2").expect("valid secret key");
        let headers = build_default_headers(&key).expect("default headers");
        reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .expect("reqwest client")
    }

    #[test]
    fn tc_6_1_0_01_query_encoding_and_empty_params() {
        assert!(ListSettlementsParams::new().to_query_pairs().is_empty());

        let gt: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().expect("valid gt");
        let lt: DateTime<Utc> = "2026-02-01T00:00:00Z".parse().expect("valid lt");
        let params = ListSettlementsParams::new()
            .page(2)
            .created_gt(gt)
            .created_lt(lt);

        let pairs = params.to_query_pairs();
        let url = reqwest::Url::parse_with_params("https://api.moyasar.com/v1/settlements", &pairs)
            .expect("valid url");
        assert_eq!(
            url.query().expect("query present"),
            "page=2&created%5Bgt%5D=2026-01-01T00%3A00%3A00Z&created%5Blt%5D=2026-02-01T00%3A00%3A00Z"
        );
    }

    #[tokio::test]
    async fn ac_us_6_1_0_01_wiremock_list_pagination_and_error_handling() {
        let server = MockServer::start().await;
        let gt: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().expect("valid gt");

        Mock::given(method("GET"))
            .and(path("/v1/settlements"))
            .and(query_param("page", "1"))
            .and(query_param("created[gt]", "2026-01-01T00:00:00Z"))
            .and(header(MOYASAR_VERSION_HEADER, API_VERSION))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "settlements": [{
                    "id": "stl_01J9TEST",
                    "recipient_type": "entity",
                    "currency": "SAR",
                    "source_currency": "SAR",
                    "amount": 100000,
                    "fee": 2500,
                    "tax": 375,
                    "created_at": "2026-01-15T12:00:00Z"
                }],
                "meta": {
                    "current_page": 1,
                    "next_page": 2,
                    "prev_page": null,
                    "total_pages": 2,
                    "total_count": 2
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let http = test_http_client();
        let params = ListSettlementsParams::new().page(1).created_gt(gt);
        let url = format!("{}{SETTLEMENTS_PATH}", server.uri());
        let page: PaginatedList<Value> = send_get(&http, &url, 0, &params.to_query_pairs())
            .await
            .expect("list succeeds");

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0]["id"], "stl_01J9TEST");
        assert_eq!(page.meta.current_page, 1);
        assert_eq!(page.meta.next_page, Some(2));
        assert_eq!(page.meta.total_count, 2);

        Mock::given(method("GET"))
            .and(path("/v1/settlements"))
            .and(query_param("page", "99"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "type": "authentication_error",
                "message": "Invalid API key"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let err_params = ListSettlementsParams::new().page(99);
        let err = send_get::<PaginatedList<Value>>(&http, &url, 2, &err_params.to_query_pairs())
            .await
            .expect_err("401 must not retry");
        assert!(!err.is_retryable());
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::AuthenticationError,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn wiremock_retries_503_then_recovers() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/settlements"))
            .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/settlements"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "settlements": [],
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

        let http = test_http_client();
        let url = format!("{}{SETTLEMENTS_PATH}", server.uri());
        let page: PaginatedList<Value> = send_get(&http, &url, 2, &[])
            .await
            .expect("recovers after 503");
        assert!(page.items.is_empty());
        assert_eq!(page.meta.total_count, 0);
    }
}
