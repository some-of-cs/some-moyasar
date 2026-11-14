//! Internal transaction endpoints (`ListInternalTransactionsParams`, `InternalTransactionsService::list`).

use chrono::{DateTime, Utc};

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::pagination::PaginatedList;
use crate::kernel::transport::execute_with_retry;
use crate::kernel::types::Currency;
use crate::resources::settlements::internal_txn_models::{
    InternalTransaction, InternalTransactionId,
};
use crate::resources::settlements::service::InternalTransactionsService;

/// Query filter and pagination parameters for `GET /v1/internal_transactions`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListInternalTransactionsParams {
    /// 1-based page number.
    pub page: Option<u32>,
    /// Filter by exact internal transaction identifier.
    pub id: Option<InternalTransactionId>,
    /// Filter by ISO 4217 currency code.
    pub currency: Option<Currency>,
    /// Filter internal transactions created strictly after this UTC timestamp (`created_at[gt]`).
    pub created_gt: Option<DateTime<Utc>>,
    /// Filter internal transactions created strictly before this UTC timestamp (`created_at[lt]`).
    pub created_lt: Option<DateTime<Utc>>,
    /// Filter internal transactions updated strictly after this UTC timestamp (`updated_at[gt]`).
    pub updated_gt: Option<DateTime<Utc>>,
    /// Filter internal transactions updated strictly before this UTC timestamp (`updated_at[lt]`).
    pub updated_lt: Option<DateTime<Utc>>,
    /// Filter internal transactions settled strictly after this UTC timestamp (`settled_at[gt]`).
    pub settled_gt: Option<DateTime<Utc>>,
    /// Filter internal transactions settled strictly before this UTC timestamp (`settled_at[lt]`).
    pub settled_lt: Option<DateTime<Utc>>,
}

impl ListInternalTransactionsParams {
    /// Creates an empty [`ListInternalTransactionsParams`] with no filters set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Alias for [`ListInternalTransactionsParams::new`] for fluent builder-style construction.
    #[must_use]
    pub fn builder() -> Self {
        Self::default()
    }

    /// Sets the 1-based `page` query parameter.
    #[must_use]
    pub fn page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }

    /// Sets the `id` filter query parameter.
    #[must_use]
    pub fn id(mut self, id: InternalTransactionId) -> Self {
        self.id = Some(id);
        self
    }

    /// Sets the `currency` filter query parameter.
    #[must_use]
    pub fn currency(mut self, currency: Currency) -> Self {
        self.currency = Some(currency);
        self
    }

    /// Sets the `created_at[gt]` lower-bound UTC timestamp filter.
    #[must_use]
    pub fn created_gt(mut self, ts: DateTime<Utc>) -> Self {
        self.created_gt = Some(ts);
        self
    }

    /// Sets the `created_at[lt]` upper-bound UTC timestamp filter.
    #[must_use]
    pub fn created_lt(mut self, ts: DateTime<Utc>) -> Self {
        self.created_lt = Some(ts);
        self
    }

    /// Sets the `updated_at[gt]` lower-bound UTC timestamp filter.
    #[must_use]
    pub fn updated_gt(mut self, ts: DateTime<Utc>) -> Self {
        self.updated_gt = Some(ts);
        self
    }

    /// Sets the `updated_at[lt]` upper-bound UTC timestamp filter.
    #[must_use]
    pub fn updated_lt(mut self, ts: DateTime<Utc>) -> Self {
        self.updated_lt = Some(ts);
        self
    }

    /// Sets the `settled_at[gt]` lower-bound UTC timestamp filter.
    #[must_use]
    pub fn settled_gt(mut self, ts: DateTime<Utc>) -> Self {
        self.settled_gt = Some(ts);
        self
    }

    /// Sets the `settled_at[lt]` upper-bound UTC timestamp filter.
    #[must_use]
    pub fn settled_lt(mut self, ts: DateTime<Utc>) -> Self {
        self.settled_lt = Some(ts);
        self
    }

    /// Encodes all configured filters into key/value query parameter pairs.
    #[must_use]
    pub fn to_query_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        push_opt_str(&mut pairs, "page", self.page.map(|p| p.to_string()));
        push_opt_str(&mut pairs, "id", self.id.as_ref().map(ToString::to_string));
        push_opt_str(
            &mut pairs,
            "currency",
            self.currency.map(|c| format!("{c:?}")),
        );
        push_opt_ts(&mut pairs, "created_at[gt]", self.created_gt);
        push_opt_ts(&mut pairs, "created_at[lt]", self.created_lt);
        push_opt_ts(&mut pairs, "updated_at[gt]", self.updated_gt);
        push_opt_ts(&mut pairs, "updated_at[lt]", self.updated_lt);
        push_opt_ts(&mut pairs, "settled_at[gt]", self.settled_gt);
        push_opt_ts(&mut pairs, "settled_at[lt]", self.settled_lt);
        pairs
    }
}

fn push_opt_str(pairs: &mut Vec<(String, String)>, key: &str, value: Option<String>) {
    if let Some(val) = value {
        pairs.push((key.to_owned(), val));
    }
}

fn push_opt_ts(pairs: &mut Vec<(String, String)>, key: &str, ts: Option<DateTime<Utc>>) {
    if let Some(dt) = ts {
        pairs.push((key.to_owned(), dt.to_rfc3339()));
    }
}

impl<'a, K: SecretAuthKey> InternalTransactionsService<'a, K> {
    /// Lists internal transactions matching [`ListInternalTransactionsParams`] via
    /// `GET /v1/internal_transactions`.
    ///
    /// Restricted at compile time to [`SecretAuthKey`] clients (`SecretKey`).
    /// Attempting to call `list` on a `PublishableKey` client fails to compile:
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::settlements::internal_transactions::ListInternalTransactionsParams;
    ///
    /// # async fn run() {
    /// let key = PublishableKey::new("pk_test_YOUR_KEY").unwrap();
    /// let client = MoyasarClient::new(key).unwrap();
    /// let _ = client
    ///     .internal_transactions()
    ///     .list(&ListInternalTransactionsParams::default())
    ///     .await;
    /// # }
    /// ```
    pub async fn list(
        &self,
        params: &ListInternalTransactionsParams,
    ) -> Result<PaginatedList<InternalTransaction>, MoyasarError> {
        let base_url = self.client().config().base_url.trim_end_matches('/');
        let url = format!("{base_url}/v1/internal_transactions");
        let query_pairs = params.to_query_pairs();
        let http = self.client().http();
        let retries = self.client().config().max_network_retries;

        execute_with_retry(retries, || async {
            let response = http.get(&url).query(&query_pairs).send().await?;
            parse_list_response(response).await
        })
        .await
    }
}

async fn parse_list_response(
    response: reqwest::Response,
) -> Result<PaginatedList<InternalTransaction>, MoyasarError> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(MoyasarError::from_api_response(status.as_u16(), &body));
    }
    serde_json::from_str::<PaginatedList<InternalTransaction>>(&body).map_err(|err| {
        MoyasarError::ServerError {
            status: status.as_u16(),
            message: format!("failed to decode PaginatedList<InternalTransaction>: {err}"),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn tc_6_2_0_03_currency_and_settled_at_query_encoding() {
        let settled_gt = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
        let settled_lt = Utc.with_ymd_and_hms(2026, 11, 14, 23, 59, 59).unwrap();
        let params = ListInternalTransactionsParams::builder()
            .page(2)
            .currency(Currency::SAR)
            .settled_gt(settled_gt)
            .settled_lt(settled_lt);
        let pairs = params.to_query_pairs();
        assert_eq!(
            pairs,
            vec![
                ("page".to_owned(), "2".to_owned()),
                ("currency".to_owned(), "SAR".to_owned()),
                (
                    "settled_at[gt]".to_owned(),
                    "2026-11-01T00:00:00+00:00".to_owned()
                ),
                (
                    "settled_at[lt]".to_owned(),
                    "2026-11-14T23:59:59+00:00".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn encodes_created_and_updated_timestamp_ranges_and_empty_default() {
        assert!(ListInternalTransactionsParams::new().to_query_pairs().is_empty());

        let t1 = Utc.with_ymd_and_hms(2026, 11, 1, 10, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 11, 2, 10, 0, 0).unwrap();
        let t3 = Utc.with_ymd_and_hms(2026, 11, 3, 10, 0, 0).unwrap();
        let t4 = Utc.with_ymd_and_hms(2026, 11, 4, 10, 0, 0).unwrap();

        let params = ListInternalTransactionsParams::builder()
            .currency(Currency::USD)
            .created_gt(t1)
            .created_lt(t2)
            .updated_gt(t3)
            .updated_lt(t4);
        let pairs = params.to_query_pairs();
        assert_eq!(
            pairs,
            vec![
                ("currency".to_owned(), "USD".to_owned()),
                (
                    "created_at[gt]".to_owned(),
                    "2026-11-01T10:00:00+00:00".to_owned()
                ),
                (
                    "created_at[lt]".to_owned(),
                    "2026-11-02T10:00:00+00:00".to_owned()
                ),
                (
                    "updated_at[gt]".to_owned(),
                    "2026-11-03T10:00:00+00:00".to_owned()
                ),
                (
                    "updated_at[lt]".to_owned(),
                    "2026-11-04T10:00:00+00:00".to_owned()
                ),
            ]
        );
    }

    #[tokio::test]
    async fn list_maps_api_error_response_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/internal_transactions"))
            .and(query_param("currency", "SAR"))
            .and(query_param("settled_at[gt]", "2026-11-01T00:00:00+00:00"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "type": "authentication_error",
                "message": "Invalid key"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let settled_gt = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
        let params = ListInternalTransactionsParams::builder()
            .currency(Currency::SAR)
            .settled_gt(settled_gt);

        let url = format!("{}/v1/internal_transactions", server.uri());
        let query_pairs = params.to_query_pairs();
        let http = reqwest::Client::new();
        let err = execute_with_retry(0, || async {
            let resp = http.get(&url).query(&query_pairs).send().await?;
            parse_list_response(resp).await
        })
        .await
        .expect_err("401 must map to MoyasarError::Api");

        assert!(!err.is_retryable());
        assert!(matches!(err, MoyasarError::Api { .. }));
    }
}
