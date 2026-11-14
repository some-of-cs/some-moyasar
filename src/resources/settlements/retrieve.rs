//! Settlement retrieval (`GET /v1/settlements/{id}`) and net-amount reconciliation helpers.

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::transport::execute_with_retry;
use crate::kernel::types::Amount;
use crate::resources::settlements::models::{Settlement, SettlementId};
use crate::resources::settlements::service::SettlementsService;

const SETTLEMENTS_PATH: &str = "/v1/settlements";

impl Settlement {
    /// Computes the net settled amount (`amount - fee - tax`) in minor units.
    ///
    /// Uses checked minor-unit subtraction and falls back to `0` if arithmetic
    /// underflow occurs. Use [`Settlement::checked_net`] to observe underflow
    /// explicitly.
    ///
    /// # Examples
    ///
    /// ```
    /// use some_moyasar::kernel::types::Amount;
    /// use some_moyasar::resources::settlements::models::Settlement;
    ///
    /// let settlement: Settlement = serde_json::from_str(
    ///     r#"{
    ///         "id": "550e8400-e29b-41d4-a716-446655440000",
    ///         "recipient_type": "entity",
    ///         "currency": "SAR",
    ///         "source_currency": "SAR",
    ///         "amount": 100000,
    ///         "fee": 2500,
    ///         "tax": 375,
    ///         "created_at": "2026-11-14T00:00:00Z"
    ///     }"#,
    /// )
    /// .unwrap();
    ///
    /// assert_eq!(settlement.net(), Amount::from_minor(97_125));
    /// ```
    #[must_use]
    pub fn net(&self) -> Amount {
        let minor = self
            .amount
            .minor_units()
            .checked_sub(self.fee.minor_units())
            .and_then(|v| v.checked_sub(self.tax.minor_units()))
            .unwrap_or(0);
        Amount::from_minor(minor)
    }

    /// Computes the net settled amount (`amount - fee - tax`), returning `None`
    /// on minor-unit arithmetic underflow.
    #[must_use]
    pub fn checked_net(&self) -> Option<Amount> {
        self.amount
            .minor_units()
            .checked_sub(self.fee.minor_units())
            .and_then(|v| v.checked_sub(self.tax.minor_units()))
            .map(Amount::from_minor)
    }
}

impl<'a, K: SecretAuthKey> SettlementsService<'a, K> {
    /// Retrieves a single settlement by its [`SettlementId`] via `GET /v1/settlements/{id}`.
    ///
    /// Restricted at compile time to [`SecretAuthKey`] clients (`SecretKey`).
    /// Attempting to call `retrieve` on a `PublishableKey` client fails to compile:
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::settlements::models::SettlementId;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let pk = PublishableKey::new("pk_test_abc123")?;
    /// let client = MoyasarClient::new(pk)?;
    /// let id = SettlementId::new("550e8400-e29b-41d4-a716-446655440000");
    /// let _settlement = client.settlements().retrieve(&id).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::Api`] with
    /// [`ApiErrorType::RecordNotFound`](crate::kernel::error::ApiErrorType::RecordNotFound)
    /// when the settlement does not exist (HTTP 404), or a transport/server error on failure.
    pub async fn retrieve(&self, id: &SettlementId) -> Result<Settlement, MoyasarError> {
        let client = self.client();
        retrieve_settlement(
            client.http(),
            &client.config().base_url,
            client.config().max_network_retries,
            id,
        )
        .await
    }
}

async fn retrieve_settlement(
    http: &reqwest::Client,
    base_url: &str,
    max_network_retries: u32,
    id: &SettlementId,
) -> Result<Settlement, MoyasarError> {
    id.validate()?;
    let url = format!("{}{SETTLEMENTS_PATH}/{id}", base_url.trim_end_matches('/'));
    execute_with_retry(max_network_retries, || send_retrieve_request(http, &url)).await
}

async fn send_retrieve_request(
    http: &reqwest::Client,
    url: &str,
) -> Result<Settlement, MoyasarError> {
    let response = http.get(url).send().await.map_err(map_reqwest_error)?;
    let status = response.status();
    if status.is_success() {
        return response
            .json::<Settlement>()
            .await
            .map_err(map_reqwest_error);
    }
    let body = response.text().await.unwrap_or_default();
    Err(MoyasarError::from_api_response(status.as_u16(), &body))
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
    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::client::MoyasarClient;
    use crate::kernel::error::ApiErrorType;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sample_settlement(amount: i64, fee: i64, tax: i64) -> Settlement {
        serde_json::from_value(json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "recipient_type": "entity",
            "currency": "SAR",
            "source_currency": "SAR",
            "invoicing_currency": "USD",
            "amount": amount,
            "fee": fee,
            "tax": tax,
            "invoicing_fee": 667,
            "invoicing_tax": 100,
            "invoicing_ex_rate": 3.75,
            "reference": "STL-2026-001",
            "settlement_count": 12,
            "invoice_url": "https://api.moyasar.com/reports/invoice.pdf",
            "csv_list_url": "https://api.moyasar.com/reports/list.csv",
            "pdf_list_url": "https://api.moyasar.com/reports/list.pdf",
            "created_at": "2026-11-14T00:00:00Z"
        }))
        .expect("sample settlement deserializes")
    }

    #[test]
    fn tc_6_1_0_02_net_math_and_checked_underflow() {
        let settlement = sample_settlement(100_000, 2_500, 375);
        assert_eq!(settlement.net(), Amount::from_minor(97_125));
        assert_eq!(settlement.checked_net(), Some(Amount::from_minor(97_125)));

        let underflow_fee = sample_settlement(i64::MIN, 1, 0);
        assert_eq!(underflow_fee.checked_net(), None);
        assert_eq!(underflow_fee.net(), Amount::from_minor(0));

        let underflow_tax = sample_settlement(i64::MIN + 5, 5, 1);
        assert_eq!(underflow_tax.checked_net(), None);
        assert_eq!(underflow_tax.net(), Amount::from_minor(0));
    }

    #[tokio::test]
    async fn retrieve_returns_settlement_and_maps_404_to_record_not_found() {
        let server = MockServer::start().await;
        let id = SettlementId::new("550e8400-e29b-41d4-a716-446655440000");

        Mock::given(method("GET"))
            .and(path("/v1/settlements/550e8400-e29b-41d4-a716-446655440000"))
            .and(header("moyasar-version", "2024-01-01"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "550e8400-e29b-41d4-a716-446655440000",
                "recipient_type": "entity",
                "currency": "SAR",
                "source_currency": "SAR",
                "amount": 150_000,
                "fee": 3_000,
                "tax": 450,
                "created_at": "2026-11-14T00:00:00Z"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_settlement_123").expect("valid secret key");
        let client = MoyasarClient::builder(key)
            .max_network_retries(1)
            .build()
            .expect("client builds");

        let settlement = retrieve_settlement(
            client.http(),
            &server.uri(),
            client.config().max_network_retries,
            &id,
        )
        .await
        .expect("retrieves settlement");

        assert_eq!(settlement.id, id);
        assert_eq!(settlement.net(), Amount::from_minor(146_550));

        Mock::given(method("GET"))
            .and(path("/v1/settlements/stl_missing_404"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "type": "record_not_found",
                "message": "Settlement not found"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let missing_id = SettlementId::new("stl_missing_404");
        let err = retrieve_settlement(
            client.http(),
            &server.uri(),
            client.config().max_network_retries,
            &missing_id,
        )
        .await
        .expect_err("404 maps to RecordNotFound");

        assert!(!err.is_retryable());
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::RecordNotFound,
                ref message,
                ..
            } if message == "Settlement not found"
        ));
    }
}
