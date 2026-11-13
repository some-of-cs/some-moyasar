//! Filtered payment listing endpoint (`GET /v1/payments`) for `PaymentsService` (US-2.1.0 Unit 5).

use core::fmt::Write as _;
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::pagination::PaginatedList;
use crate::kernel::transport::execute_with_retry;
use crate::resources::payments::models::{Payment, PaymentStatus};
use crate::resources::payments::service::PaymentsService;

/// Query filter and pagination parameters for `GET /v1/payments`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListParams {
    /// 1-based page number.
    pub page: Option<u32>,
    /// Filter by exact payment identifier.
    pub id: Option<String>,
    /// Filter by payment lifecycle status.
    pub status: Option<PaymentStatus>,
    /// Filter payments created strictly after this UTC timestamp (`created[gt]`).
    pub created_gt: Option<DateTime<Utc>>,
    /// Filter payments created strictly before this UTC timestamp (`created[lt]`).
    pub created_lt: Option<DateTime<Utc>>,
    /// Filter payments by metadata key/value pairs (`metadata[key]=value`).
    pub metadata: BTreeMap<String, String>,
}

impl ListParams {
    /// Creates an empty [`ListParams`] with no filters set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Alias for [`ListParams::new`] for fluent builder-style construction.
    pub fn builder() -> Self {
        Self::default()
    }

    /// Sets the 1-based `page` query parameter.
    pub fn page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }

    /// Sets the `id` filter query parameter.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Sets the `status` filter query parameter.
    pub fn status(mut self, status: PaymentStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// Sets the `created[gt]` lower-bound UTC timestamp filter.
    pub fn created_gt(mut self, ts: DateTime<Utc>) -> Self {
        self.created_gt = Some(ts);
        self
    }

    /// Sets the `created[lt]` upper-bound UTC timestamp filter.
    pub fn created_lt(mut self, ts: DateTime<Utc>) -> Self {
        self.created_lt = Some(ts);
        self
    }

    /// Inserts a `metadata[key]=value` filter entry.
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Serializes all configured filters into a percent-encoded query string.
    ///
    /// Metadata entries use Moyasar's bracket notation (`metadata[key]=value`)
    /// with both keys and values percent-encoded to prevent query injection.
    pub fn to_query_string(&self) -> String {
        let mut pairs: Vec<String> = Vec::new();
        if let Some(page) = self.page {
            pairs.push(format!("page={page}"));
        }
        if let Some(id) = &self.id {
            pairs.push(format!("id={}", percent_encode(id)));
        }
        if let Some(status) = &self.status {
            pairs.push(format!("status={}", percent_encode(status.as_str())));
        }
        if let Some(gt) = &self.created_gt {
            pairs.push(format!("created[gt]={}", percent_encode(&gt.to_rfc3339())));
        }
        if let Some(lt) = &self.created_lt {
            pairs.push(format!("created[lt]={}", percent_encode(&lt.to_rfc3339())));
        }
        for (k, v) in &self.metadata {
            pairs.push(format!(
                "metadata[{}]={}",
                percent_encode(k),
                percent_encode(v)
            ));
        }
        pairs.join("&")
    }
}

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        let is_unreserved =
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~');
        if is_unreserved {
            out.push(byte as char);
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

impl<'a, K: SecretAuthKey> PaymentsService<'a, K> {
    /// Lists payments matching [`ListParams`] via `GET /v1/payments`.
    ///
    /// Restricted at compile time to [`SecretAuthKey`] clients (`SecretKey`).
    /// Attempting to call `list` on a `PublishableKey` client fails to compile:
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::payments::list::ListParams;
    ///
    /// # async fn run() {
    /// let key = PublishableKey::new("pk_test_YOUR_KEY").unwrap();
    /// let client = MoyasarClient::new(key).unwrap();
    /// let _ = client.payments().list(&ListParams::default()).await;
    /// # }
    /// ```
    pub async fn list(&self, params: &ListParams) -> Result<PaginatedList<Payment>, MoyasarError> {
        let qs = params.to_query_string();
        let url = if qs.is_empty() {
            format!("{}/v1/payments", self.client().config().base_url)
        } else {
            format!("{}/v1/payments?{qs}", self.client().config().base_url)
        };
        let http = self.client().http();
        let retries = self.client().config().max_network_retries;

        execute_with_retry(retries, || async {
            let response = http.get(&url).send().await?;
            parse_list_response(response).await
        })
        .await
    }
}

async fn parse_list_response(
    response: reqwest::Response,
) -> Result<PaginatedList<Payment>, MoyasarError> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(MoyasarError::from_api_response(status.as_u16(), &body));
    }
    serde_json::from_str::<PaginatedList<Payment>>(&body).map_err(|err| MoyasarError::ServerError {
        status: status.as_u16(),
        message: format!("failed to decode PaginatedList<Payment>: {err}"),
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
    fn tc_2_1_0_04_query_string_golden_vector() {
        let gt = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap();
        let params = ListParams::builder()
            .page(2)
            .status(PaymentStatus::Paid)
            .created_gt(gt);
        assert_eq!(
            params.to_query_string(),
            "page=2&status=paid&created[gt]=2026-11-01T00%3A00%3A00%2B00%3A00"
        );
    }

    #[test]
    fn tc_2_1_0_05_metadata_bracket_notation_and_percent_encoding() {
        let params = ListParams::builder()
            .metadata("order_id", "ORD-1")
            .metadata("a&b=c", "100% ok");
        let qs = params.to_query_string();
        assert!(qs.contains("metadata[order_id]=ORD-1"));
        assert!(qs.contains("metadata[a%26b%3Dc]=100%25%20ok"));
    }

    #[tokio::test]
    async fn list_deserializes_paginated_payments_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/payments"))
            .and(query_param("page", "2"))
            .and(query_param("status", "paid"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "payments": [{
                    "id": "pay_01J9LIST",
                    "status": "paid",
                    "amount": 15000,
                    "amount_format": "150.00 SAR",
                    "fee": 350,
                    "fee_format": "3.50 SAR",
                    "refunded": 0,
                    "refunded_format": "0.00 SAR",
                    "captured": 15000,
                    "captured_format": "150.00 SAR",
                    "currency": "SAR",
                    "source": { "type": "creditcard" },
                    "created_at": "2026-11-13T10:00:00Z",
                    "updated_at": "2026-11-13T10:01:00Z"
                }],
                "meta": {
                    "current_page": 2,
                    "next_page": null,
                    "prev_page": 1,
                    "total_pages": 2,
                    "total_count": 1
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let params = ListParams::builder().page(2).status(PaymentStatus::Paid);
        let url = format!("{}/v1/payments?{}", server.uri(), params.to_query_string());
        let http = reqwest::Client::new();
        let page = execute_with_retry(1, || async {
            let resp = http.get(&url).send().await?;
            parse_list_response(resp).await
        })
        .await
        .expect("lists payments");

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, "pay_01J9LIST");
        assert_eq!(page.meta.current_page, 2);
    }
}
