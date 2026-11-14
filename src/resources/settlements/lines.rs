//! Settlement lines (`SettlementLine`, `ListSettlementLinesParams`, `SettlementsService::list_lines`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::pagination::PaginatedList;
use crate::kernel::transport::execute_with_retry;
use crate::kernel::types::{Amount, Currency};
use crate::resources::settlements::models::SettlementId;
use crate::resources::settlements::service::SettlementsService;

/// Individual line item belonging to a Moyasar settlement (`GET /v1/settlements/:id/lines`).
///
/// # Examples
///
/// ```
/// use some_moyasar::kernel::types::{Amount, Currency};
/// use some_moyasar::resources::settlements::lines::SettlementLine;
///
/// let raw = r#"{
///     "payment_id": "550e8400-e29b-41d4-a716-446655440000",
///     "type": "payment",
///     "currency": "SAR",
///     "amount": 10000,
///     "fee": 250,
///     "tax": 38,
///     "reference": "INV-1001",
///     "settled_at": "2026-11-13T10:00:00Z"
/// }"#;
///
/// let line: SettlementLine = serde_json::from_str(raw).unwrap();
/// assert_eq!(line.line_type.as_deref(), Some("payment"));
/// assert_eq!(line.currency, Currency::SAR);
/// assert_eq!(line.amount, Amount::from_minor(10_000));
/// assert_eq!(line.fee, Amount::from_minor(250));
/// assert_eq!(line.tax, Amount::from_minor(38));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementLine {
    /// Associated payment UUID, when the line corresponds to a payment.
    #[serde(default)]
    pub payment_id: Option<Uuid>,
    /// Line item classification returned under JSON key `"type"`.
    #[serde(rename = "type", default)]
    pub line_type: Option<String>,
    /// ISO 4217 currency code for the line amounts.
    pub currency: Currency,
    /// Gross transaction amount in the smallest currency unit.
    pub amount: Amount,
    /// Processing fee deducted in the smallest currency unit.
    pub fee: Amount,
    /// Tax applied to the fee in the smallest currency unit.
    pub tax: Amount,
    /// Optional merchant or gateway reference string.
    #[serde(default)]
    pub reference: Option<String>,
    /// UTC timestamp when this line item was settled.
    #[serde(default)]
    pub settled_at: Option<DateTime<Utc>>,
}

/// Pagination parameters for `GET /v1/settlements/:id/lines`.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::settlements::lines::ListSettlementLinesParams;
///
/// let params = ListSettlementLinesParams::new().page(2);
/// assert_eq!(
///     params.to_query_pairs(),
///     vec![("page".to_owned(), "2".to_owned())]
/// );
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListSettlementLinesParams {
    /// Optional 1-based page number.
    pub page: Option<u32>,
}

impl ListSettlementLinesParams {
    /// Creates an empty [`ListSettlementLinesParams`] with no page filter set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the 1-based `page` query parameter.
    #[must_use]
    pub fn page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }

    /// Encodes configured parameters into `(key, value)` query pairs.
    #[must_use]
    pub fn to_query_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        if let Some(page) = self.page {
            pairs.push(("page".to_owned(), page.to_string()));
        }
        pairs
    }
}

fn build_lines_url(
    base_url: &str,
    id: &SettlementId,
    params: &ListSettlementLinesParams,
) -> String {
    let base = base_url.trim_end_matches('/');
    let pairs = params.to_query_pairs();
    if pairs.is_empty() {
        return format!("{base}/v1/settlements/{}/lines", id.as_str());
    }
    let query: Vec<String> = pairs
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    format!(
        "{base}/v1/settlements/{}/lines?{}",
        id.as_str(),
        query.join("&")
    )
}

impl<'a, K: SecretAuthKey> SettlementsService<'a, K> {
    /// Lists settlement lines for a settlement via `GET /v1/settlements/:id/lines`.
    ///
    /// Restricted at compile time to [`SecretAuthKey`] clients (`SecretKey`).
    /// Attempting to call `list_lines` on a `PublishableKey` client fails to compile:
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::settlements::lines::ListSettlementLinesParams;
    /// use some_moyasar::resources::settlements::models::SettlementId;
    ///
    /// # async fn run() {
    /// let key = PublishableKey::new("pk_test_YOUR_KEY").unwrap();
    /// let client = MoyasarClient::new(key).unwrap();
    /// let id = SettlementId::new("stl_01J9XYZ").unwrap();
    /// let _ = client
    ///     .settlements()
    ///     .list_lines(&id, &ListSettlementLinesParams::default())
    ///     .await;
    /// # }
    /// ```
    pub async fn list_lines(
        &self,
        id: &SettlementId,
        params: &ListSettlementLinesParams,
    ) -> Result<PaginatedList<SettlementLine>, MoyasarError> {
        let url = build_lines_url(&self.client().config().base_url, id, params);
        let http = self.client().http();
        let retries = self.client().config().max_network_retries;

        execute_with_retry(retries, || async {
            let response = http.get(&url).send().await?;
            parse_lines_response(response).await
        })
        .await
    }
}

async fn parse_lines_response(
    response: reqwest::Response,
) -> Result<PaginatedList<SettlementLine>, MoyasarError> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(MoyasarError::from_api_response(status.as_u16(), &body));
    }
    serde_json::from_str::<PaginatedList<SettlementLine>>(&body).map_err(|err| {
        MoyasarError::ServerError {
            status: status.as_u16(),
            message: format!("failed to decode PaginatedList<SettlementLine>: {err}"),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::error::ApiErrorType;
    use chrono::TimeZone as _;
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn tc_6_1_0_05_deserializes_settlement_line_and_paginated_list() {
        let raw = json!({
            "lines": [
                {
                    "payment_id": "550e8400-e29b-41d4-a716-446655440000",
                    "type": "payment",
                    "currency": "SAR",
                    "amount": 15000,
                    "fee": 350,
                    "tax": 53,
                    "reference": "ORD-42",
                    "settled_at": "2026-11-13T12:30:00Z"
                }
            ],
            "meta": {
                "current_page": 1,
                "next_page": 2,
                "prev_page": null,
                "total_pages": 3,
                "total_count": 25
            }
        });

        let page: PaginatedList<SettlementLine> =
            serde_json::from_value(raw).expect("valid paginated settlement lines");
        assert_eq!(page.items.len(), 1);

        let line = &page.items[0];
        assert_eq!(
            line.payment_id,
            Some(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap())
        );
        assert_eq!(line.line_type.as_deref(), Some("payment"));
        assert_eq!(line.currency, Currency::SAR);
        assert_eq!(line.amount, Amount::from_minor(15_000));
        assert_eq!(line.fee, Amount::from_minor(350));
        assert_eq!(line.tax, Amount::from_minor(53));
        assert_eq!(line.reference.as_deref(), Some("ORD-42"));
        assert_eq!(
            line.settled_at,
            Some(Utc.with_ymd_and_hms(2026, 11, 13, 12, 30, 0).unwrap())
        );

        assert_eq!(page.meta.current_page, 1);
        assert_eq!(page.meta.next_page, Some(2));
        assert_eq!(page.meta.total_pages, 3);
        assert_eq!(page.meta.total_count, 25);

        let serialized = serde_json::to_string(line).expect("serializes cleanly");
        let round_tripped: SettlementLine =
            serde_json::from_str(&serialized).expect("round-trips cleanly");
        assert_eq!(&round_tripped, line);
    }

    #[test]
    fn tc_6_1_0_05_deserializes_minimal_settlement_line_with_defaults() {
        let raw = r#"{
            "currency": "USD",
            "amount": 5000,
            "fee": 150,
            "tax": 0
        }"#;

        let line: SettlementLine =
            serde_json::from_str(raw).expect("minimal settlement line deserializes");
        assert!(line.payment_id.is_none());
        assert!(line.line_type.is_none());
        assert_eq!(line.currency, Currency::USD);
        assert_eq!(line.amount, Amount::from_minor(5_000));
        assert_eq!(line.fee, Amount::from_minor(150));
        assert_eq!(line.tax, Amount::from_minor(0));
        assert!(line.reference.is_none());
        assert!(line.settled_at.is_none());
    }

    #[test]
    fn tc_6_1_0_05_encodes_list_settlement_lines_query_pairs() {
        let empty = ListSettlementLinesParams::new();
        assert!(empty.to_query_pairs().is_empty());

        let paged = ListSettlementLinesParams::new().page(4);
        assert_eq!(
            paged.to_query_pairs(),
            vec![("page".to_owned(), "4".to_owned())]
        );
    }

    #[tokio::test]
    async fn tc_6_1_0_05_parses_paginated_lines_and_404_via_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/settlements/stl_01J9LINES/lines"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "lines": [
                    {
                        "payment_id": null,
                        "type": "refund",
                        "currency": "SAR",
                        "amount": -2500,
                        "fee": 0,
                        "tax": 0,
                        "reference": "REF-9",
                        "settled_at": "2026-11-14T01:00:00Z"
                    }
                ],
                "meta": {
                    "current_page": 2,
                    "next_page": null,
                    "prev_page": 1,
                    "total_pages": 2,
                    "total_count": 2
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let params = ListSettlementLinesParams::new().page(2);
        let pairs = params.to_query_pairs();
        let url = format!(
            "{}/v1/settlements/stl_01J9LINES/lines?{}={}",
            server.uri(),
            pairs[0].0,
            pairs[0].1
        );
        let http = reqwest::Client::new();
        let page = execute_with_retry(1, || async {
            let resp = http.get(&url).send().await?;
            parse_lines_response(resp).await
        })
        .await
        .expect("parses paginated lines");

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].line_type.as_deref(), Some("refund"));
        assert_eq!(page.items[0].amount, Amount::from_minor(-2_500));
        assert_eq!(page.meta.current_page, 2);

        Mock::given(method("GET"))
            .and(path("/v1/settlements/stl_missing/lines"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "type": "record_not_found",
                "message": "Settlement not found"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let missing_url = format!("{}/v1/settlements/stl_missing/lines", server.uri());
        let err = execute_with_retry(1, || async {
            let resp = http.get(&missing_url).send().await?;
            parse_lines_response(resp).await
        })
        .await
        .expect_err("404 maps to RecordNotFound");

        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::RecordNotFound,
                ..
            }
        ));
    }
}
