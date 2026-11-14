//! Invoice retrieval and listing (`ListInvoicesParams`, `InvoicesService::retrieve`, `InvoicesService::list`).

use std::collections::BTreeMap;

use chrono::{DateTime, SecondsFormat, Utc};
use uuid::Uuid;

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::pagination::PaginatedList;
use crate::kernel::transport::send_get_json;
use crate::resources::invoices::InvoicesService;
use crate::resources::invoices::models::{Invoice, InvoiceStatus};

const INVOICES_PATH: &str = "/v1/invoices";

/// Query parameters for listing hosted invoices (`GET /v1/invoices`).
///
/// Supports pagination (`page`), exact `id` and `status` filtering,
/// `created[gt]` / `created[lt]` timestamp bounds, and `metadata[key]` filters.
///
/// # Examples
///
/// ```
/// use chrono::{DateTime, Utc};
/// use some_moyasar::resources::invoices::models::InvoiceStatus;
/// use some_moyasar::resources::invoices::retrieve_list::ListInvoicesParams;
///
/// let since: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
/// let params = ListInvoicesParams::new()
///     .page(2)
///     .status(InvoiceStatus::Paid)
///     .created_gt(since)
///     .metadata("order_id", "ord_123");
///
/// let pairs = params.to_query_pairs();
/// assert_eq!(
///     pairs,
///     vec![
///         ("page".to_owned(), "2".to_owned()),
///         ("status".to_owned(), "paid".to_owned()),
///         ("created[gt]".to_owned(), "2026-01-01T00:00:00Z".to_owned()),
///         ("metadata[order_id]".to_owned(), "ord_123".to_owned()),
///     ]
/// );
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListInvoicesParams {
    /// 1-based page number to fetch.
    pub page: Option<u32>,
    /// Filter by an exact invoice UUID.
    pub id: Option<Uuid>,
    /// Filter by invoice lifecycle status.
    pub status: Option<InvoiceStatus>,
    /// Filter for invoices created strictly after this UTC timestamp (`created[gt]`).
    pub created_gt: Option<DateTime<Utc>>,
    /// Filter for invoices created strictly before this UTC timestamp (`created[lt]`).
    pub created_lt: Option<DateTime<Utc>>,
    /// Filter by exact metadata key-value pairs (`metadata[key]=value`).
    pub metadata: BTreeMap<String, String>,
}

impl ListInvoicesParams {
    /// Creates an empty [`ListInvoicesParams`] with no filters set.
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

    /// Filters by a specific invoice UUID (`id`).
    #[must_use]
    pub const fn id(mut self, id: Uuid) -> Self {
        self.id = Some(id);
        self
    }

    /// Filters by [`InvoiceStatus`] (`status`).
    #[must_use]
    pub const fn status(mut self, status: InvoiceStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// Filters for invoices created after `ts` (`created[gt]`).
    #[must_use]
    pub const fn created_gt(mut self, ts: DateTime<Utc>) -> Self {
        self.created_gt = Some(ts);
        self
    }

    /// Filters for invoices created before `ts` (`created[lt]`).
    #[must_use]
    pub const fn created_lt(mut self, ts: DateTime<Utc>) -> Self {
        self.created_lt = Some(ts);
        self
    }

    /// Adds a `metadata[key]=value` filter entry.
    #[must_use]
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Converts all configured filters into ordered `(key, value)` query pairs.
    #[must_use]
    pub fn to_query_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        self.push_scalar_filters(&mut pairs);
        self.push_range_and_metadata_filters(&mut pairs);
        pairs
    }

    fn push_scalar_filters(&self, pairs: &mut Vec<(String, String)>) {
        if let Some(page) = self.page {
            pairs.push(("page".to_owned(), page.to_string()));
        }
        if let Some(id) = self.id {
            pairs.push(("id".to_owned(), id.to_string()));
        }
        if let Some(status) = self.status.and_then(status_query_value) {
            pairs.push(("status".to_owned(), status.to_owned()));
        }
    }

    fn push_range_and_metadata_filters(&self, pairs: &mut Vec<(String, String)>) {
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
        for (key, value) in &self.metadata {
            pairs.push((format!("metadata[{key}]"), value.clone()));
        }
    }
}

const fn status_query_value(status: InvoiceStatus) -> Option<&'static str> {
    match status {
        InvoiceStatus::Created => Some("created"),
        InvoiceStatus::Paid => Some("paid"),
        InvoiceStatus::Canceled => Some("canceled"),
        InvoiceStatus::Expired => Some("expired"),
        InvoiceStatus::Unknown => None,
    }
}

impl<'a, K: SecretAuthKey> InvoicesService<'a, K> {
    /// Retrieves a single hosted invoice by its UUID (`GET /v1/invoices/{id}`).
    ///
    /// Requires a [`SecretAuthKey`] (e.g., [`SecretKey`](crate::kernel::auth::SecretKey)).
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError`] on network failure, timeout, or non-2xx API
    /// response (such as `RecordNotFound` for an unknown invoice ID).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    /// use uuid::Uuid;
    ///
    /// # async fn example() -> Result<(), some_moyasar::prelude::MoyasarError> {
    /// let client = MoyasarClient::new(SecretKey::new("sk_test_example")?)?;
    /// let invoice = client.invoices().retrieve(Uuid::nil()).await?;
    /// println!("Hosted URL: {:?}", invoice.url);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// A [`PublishableKey`](crate::kernel::auth::PublishableKey) cannot call
    /// `retrieve` (`SecretAuthKey` is required):
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use uuid::Uuid;
    ///
    /// # async fn example() {
    /// let pk = PublishableKey::new("pk_test_example").unwrap();
    /// let client = MoyasarClient::new(pk).unwrap();
    /// let _ = client.invoices().retrieve(Uuid::nil()).await;
    /// # }
    /// ```
    pub async fn retrieve(&self, id: Uuid) -> Result<Invoice, MoyasarError> {
        let base = self.client().config().base_url.trim_end_matches('/');
        let url = format!("{base}{INVOICES_PATH}/{id}");
        send_get_json(
            self.client().http(),
            &url,
            self.client().config().max_network_retries,
            &[],
        )
        .await
    }

    /// Lists hosted invoices with optional filters and pagination (`GET /v1/invoices`).
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
    /// use some_moyasar::resources::invoices::models::InvoiceStatus;
    /// use some_moyasar::resources::invoices::retrieve_list::ListInvoicesParams;
    ///
    /// # async fn example() -> Result<(), some_moyasar::prelude::MoyasarError> {
    /// let client = MoyasarClient::new(SecretKey::new("sk_test_example")?)?;
    /// let params = ListInvoicesParams::new()
    ///     .page(1)
    ///     .status(InvoiceStatus::Paid);
    /// let page = client.invoices().list(&params).await?;
    /// println!("Total invoices: {}", page.meta.total_count);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// A [`PublishableKey`](crate::kernel::auth::PublishableKey) cannot call
    /// `list` (`SecretAuthKey` is required):
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::invoices::retrieve_list::ListInvoicesParams;
    ///
    /// # async fn example() {
    /// let pk = PublishableKey::new("pk_test_example").unwrap();
    /// let client = MoyasarClient::new(pk).unwrap();
    /// let _ = client.invoices().list(&ListInvoicesParams::new()).await;
    /// # }
    /// ```
    pub async fn list(
        &self,
        params: &ListInvoicesParams,
    ) -> Result<PaginatedList<Invoice>, MoyasarError> {
        let base = self.client().config().base_url.trim_end_matches('/');
        let url = format!("{base}{INVOICES_PATH}");
        let query = params.to_query_pairs();
        send_get_json(
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
    use crate::kernel::auth::SecretKey;
    use crate::kernel::error::ApiErrorType;
    use crate::kernel::transport::{API_VERSION, MOYASAR_VERSION_HEADER, build_default_headers};
    use serde_json::json;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_http_client() -> reqwest::Client {
        let key = SecretKey::new("sk_test_invoice_unit3").expect("valid secret key");
        let headers = build_default_headers(&key).expect("default headers");
        reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .expect("reqwest client")
    }

    fn sample_invoice_json(id: Uuid, status: &str) -> serde_json::Value {
        json!({
            "id": id,
            "status": status,
            "amount": 50000,
            "currency": "SAR",
            "description": "Enterprise plan Q4",
            "url": format!("https://moyasar.com/i/{id}"),
            "expired_at": "2026-12-01T00:00:00Z",
            "metadata": { "order_id": "ord_123" },
            "payments": [],
            "created_at": "2026-11-01T10:00:00Z"
        })
    }

    #[test]
    fn tc_4_2_0_03_query_string_byte_comparison_and_unknown_status_omitted() {
        let invoice_id = Uuid::nil();
        let gt: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().expect("valid gt");
        let lt: DateTime<Utc> = "2026-02-01T00:00:00Z".parse().expect("valid lt");

        let params = ListInvoicesParams::new()
            .page(2)
            .id(invoice_id)
            .status(InvoiceStatus::Created)
            .created_gt(gt)
            .created_lt(lt)
            .metadata("order_id", "ord 123");

        let pairs = params.to_query_pairs();
        let url = reqwest::Url::parse_with_params("https://api.moyasar.com/v1/invoices", &pairs)
            .expect("valid url");
        assert_eq!(
            url.query().expect("query present"),
            "page=2&id=00000000-0000-0000-0000-000000000000&status=created&created%5Bgt%5D=2026-01-01T00%3A00%3A00Z&created%5Blt%5D=2026-02-01T00%3A00%3A00Z&metadata%5Border_id%5D=ord+123"
        );

        let unknown_params = ListInvoicesParams::new().status(InvoiceStatus::Unknown);
        assert!(unknown_params.to_query_pairs().is_empty());
    }

    #[tokio::test]
    async fn ac_us_4_2_0_03_wiremock_list_with_filters_and_pagination() {
        let server = MockServer::start().await;
        let invoice_id = Uuid::new_v4();
        let created_gt: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().expect("valid ts");

        Mock::given(method("GET"))
            .and(path("/v1/invoices"))
            .and(query_param("page", "1"))
            .and(query_param("status", "paid"))
            .and(query_param("created[gt]", "2026-01-01T00:00:00Z"))
            .and(header(MOYASAR_VERSION_HEADER, API_VERSION))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "invoices": [sample_invoice_json(invoice_id, "paid")],
                "meta": {
                    "current_page": 1,
                    "next_page": 2,
                    "prev_page": null,
                    "total_pages": 3,
                    "total_count": 5
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let params = ListInvoicesParams::new()
            .page(1)
            .status(InvoiceStatus::Paid)
            .created_gt(created_gt);
        let http = test_http_client();
        let url = format!("{}{INVOICES_PATH}", server.uri());
        let page: PaginatedList<Invoice> = send_get(&http, &url, 0, &params.to_query_pairs())
            .await
            .expect("list succeeds");

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, invoice_id);
        assert_eq!(page.items[0].status, InvoiceStatus::Paid);
        assert_eq!(page.meta.current_page, 1);
        assert_eq!(page.meta.next_page, Some(2));
        assert_eq!(page.meta.prev_page, None);
        assert_eq!(page.meta.total_pages, 3);
        assert_eq!(page.meta.total_count, 5);
    }

    #[tokio::test]
    async fn wiremock_retrieve_invoice_and_404_error() {
        let server = MockServer::start().await;
        let invoice_id = Uuid::new_v4();
        let missing_id = Uuid::new_v4();

        Mock::given(method("GET"))
            .and(path(format!("/v1/invoices/{invoice_id}")))
            .and(header(MOYASAR_VERSION_HEADER, API_VERSION))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(sample_invoice_json(invoice_id, "created")),
            )
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!("/v1/invoices/{missing_id}")))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "type": "record_not_found",
                "message": "Invoice not found"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let http = test_http_client();
        let ok_url = format!("{}{INVOICES_PATH}/{invoice_id}", server.uri());
        let invoice: Invoice = send_get(&http, &ok_url, 2, &[])
            .await
            .expect("retrieve succeeds");
        assert_eq!(invoice.id, invoice_id);
        assert_eq!(invoice.status, InvoiceStatus::Created);
        assert_eq!(invoice.amount, crate::kernel::types::Amount(50000));

        let err_url = format!("{}{INVOICES_PATH}/{missing_id}", server.uri());
        let err = send_get::<Invoice>(&http, &err_url, 2, &[])
            .await
            .expect_err("404 must not retry");
        assert!(!err.is_retryable());
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::RecordNotFound,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn wiremock_retries_503_then_recovers() {
        let server = MockServer::start().await;
        let invoice_id = Uuid::new_v4();

        Mock::given(method("GET"))
            .and(path(format!("/v1/invoices/{invoice_id}")))
            .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!("/v1/invoices/{invoice_id}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(sample_invoice_json(invoice_id, "created")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let http = test_http_client();
        let url = format!("{}{INVOICES_PATH}/{invoice_id}", server.uri());
        let invoice: Invoice = send_get(&http, &url, 2, &[])
            .await
            .expect("recovers after 503");
        assert_eq!(invoice.id, invoice_id);
    }
}
