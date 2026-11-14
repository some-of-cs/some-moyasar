//! Aggregation transfer endpoints (`ListTransfersParams`, `ListTransferLinesParams`, `TransfersService`).

use chrono::{DateTime, SecondsFormat, Utc};

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::pagination::PaginatedList;
use crate::kernel::transport::send_get_json as send_get;
use crate::resources::settlements::service::TransfersService;
use crate::resources::settlements::transfer_models::{Transfer, TransferId, TransferLine};

const TRANSFERS_PATH: &str = "/v1/transfers";

/// Query filter and pagination parameters for listing aggregation transfers (`GET /v1/transfers`).
///
/// Supports pagination (`page`), exact `id` filtering, and `created[gt]` / `created[lt]`
/// UTC timestamp bounds.
///
/// # Examples
///
/// ```
/// use chrono::{DateTime, Utc};
/// use some_moyasar::resources::settlements::transfers::ListTransfersParams;
///
/// let gt: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
/// let params = ListTransfersParams::new().page(2).created_gt(gt);
/// assert_eq!(
///     params.to_query_pairs(),
///     vec![
///         ("page".to_owned(), "2".to_owned()),
///         ("created[gt]".to_owned(), "2026-01-01T00:00:00Z".to_owned()),
///     ]
/// );
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListTransfersParams {
    /// 1-based page number to fetch.
    pub page: Option<u32>,
    /// Filter by an exact aggregation transfer identifier (`id`).
    pub id: Option<TransferId>,
    /// Filter for transfers created strictly after this UTC timestamp (`created[gt]`).
    pub created_gt: Option<DateTime<Utc>>,
    /// Filter for transfers created strictly before this UTC timestamp (`created[lt]`).
    pub created_lt: Option<DateTime<Utc>>,
}

impl ListTransfersParams {
    /// Creates an empty [`ListTransfersParams`] with no filters set.
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

    /// Filters by a specific [`TransferId`] (`id`).
    #[must_use]
    pub fn id(mut self, id: TransferId) -> Self {
        self.id = Some(id);
        self
    }

    /// Filters for transfers created strictly after `ts` (`created[gt]`).
    #[must_use]
    pub const fn created_gt(mut self, ts: DateTime<Utc>) -> Self {
        self.created_gt = Some(ts);
        self
    }

    /// Filters for transfers created strictly before `ts` (`created[lt]`).
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

/// Pagination parameters for listing aggregation transfer lines (`GET /v1/transfers/{id}/lines`).
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::settlements::transfers::ListTransferLinesParams;
///
/// let params = ListTransferLinesParams::new().page(3);
/// assert_eq!(
///     params.to_query_pairs(),
///     vec![("page".to_owned(), "3".to_owned())]
/// );
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListTransferLinesParams {
    /// 1-based page number to fetch.
    pub page: Option<u32>,
}

impl ListTransferLinesParams {
    /// Creates an empty [`ListTransferLinesParams`] with no pagination override set.
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

    /// Converts all configured parameters into ordered `(key, value)` query pairs.
    #[must_use]
    pub fn to_query_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        if let Some(page) = self.page {
            pairs.push(("page".to_owned(), page.to_string()));
        }
        pairs
    }
}

impl<'a, K: SecretAuthKey> TransfersService<'a, K> {
    /// Lists aggregation transfers with optional filters and pagination (`GET /v1/transfers`).
    ///
    /// Requires a [`SecretAuthKey`] (e.g., [`SecretKey`](crate::kernel::auth::SecretKey)).
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError`] on network failure, timeout, or non-2xx API response.
    ///
    /// A [`PublishableKey`](crate::kernel::auth::PublishableKey) cannot call `list`
    /// (`SecretAuthKey` is required):
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::settlements::transfers::ListTransfersParams;
    ///
    /// # async fn example() {
    /// let pk = PublishableKey::new("pk_test_example").unwrap();
    /// let client = MoyasarClient::new(pk).unwrap();
    /// let _ = client.transfers().list(&ListTransfersParams::new()).await;
    /// # }
    /// ```
    pub async fn list(
        &self,
        params: &ListTransfersParams,
    ) -> Result<PaginatedList<Transfer>, MoyasarError> {
        let base = self.client().config().base_url.trim_end_matches('/');
        let url = format!("{base}{TRANSFERS_PATH}");
        let query = params.to_query_pairs();
        send_get(
            self.client().http(),
            &url,
            self.client().config().max_network_retries,
            &query,
        )
        .await
    }

    /// Retrieves a single aggregation transfer by its [`TransferId`] (`GET /v1/transfers/{id}`).
    ///
    /// Requires a [`SecretAuthKey`] (e.g., [`SecretKey`](crate::kernel::auth::SecretKey)).
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError`] on network failure, timeout, or non-2xx API response
    /// (such as [`RecordNotFound`](crate::kernel::error::ApiErrorType::RecordNotFound) for an
    /// unknown transfer ID).
    pub async fn retrieve(&self, id: &TransferId) -> Result<Transfer, MoyasarError> {
        id.validate()?;
        let base = self.client().config().base_url.trim_end_matches('/');
        let url = format!("{base}{TRANSFERS_PATH}/{id}");
        send_get(
            self.client().http(),
            &url,
            self.client().config().max_network_retries,
            &[],
        )
        .await
    }

    /// Lists constituent lines for an aggregation transfer (`GET /v1/transfers/{id}/lines`).
    ///
    /// Requires a [`SecretAuthKey`] (e.g., [`SecretKey`](crate::kernel::auth::SecretKey)).
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError`] on network failure, timeout, or non-2xx API response.
    pub async fn list_lines(
        &self,
        id: &TransferId,
        params: &ListTransferLinesParams,
    ) -> Result<PaginatedList<TransferLine>, MoyasarError> {
        id.validate()?;
        let base = self.client().config().base_url.trim_end_matches('/');
        let url = format!("{base}{TRANSFERS_PATH}/{id}/lines");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_6_2_0_01_list_transfers_params_query_encoding() {
        let gt: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().expect("valid gt");
        let lt: DateTime<Utc> = "2026-02-01T12:30:45Z".parse().expect("valid lt");
        let params = ListTransfersParams::new()
            .page(2)
            .created_gt(gt)
            .created_lt(lt);
        let pairs = params.to_query_pairs();
        assert_eq!(
            pairs,
            vec![
                ("page".to_owned(), "2".to_owned()),
                ("created[gt]".to_owned(), "2026-01-01T00:00:00Z".to_owned()),
                ("created[lt]".to_owned(), "2026-02-01T12:30:45Z".to_owned()),
            ]
        );

        let url = reqwest::Url::parse_with_params("https://api.moyasar.com/v1/transfers", &pairs)
            .expect("valid url");
        assert_eq!(
            url.query().expect("query present"),
            "page=2&created%5Bgt%5D=2026-01-01T00%3A00%3A00Z&created%5Blt%5D=2026-02-01T12%3A30%3A45Z"
        );
    }

    #[test]
    fn list_transfers_params_default_is_empty() {
        assert!(ListTransfersParams::new().to_query_pairs().is_empty());
    }

    #[test]
    fn list_transfer_lines_params_query_encoding() {
        assert!(ListTransferLinesParams::new().to_query_pairs().is_empty());
        assert_eq!(
            ListTransferLinesParams::new().page(4).to_query_pairs(),
            vec![("page".to_owned(), "4".to_owned())]
        );
    }
}
