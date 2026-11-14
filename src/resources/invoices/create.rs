//! Invoice creation (`CreateInvoiceParams`, `InvoicesService::create`, `InvoicesService::create_bulk`).

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::client::MoyasarClient;
use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::metadata::Metadata;
use crate::kernel::transport::execute_with_retry;
use crate::kernel::types::{Amount, Currency};
use crate::resources::invoices::InvoicesService;
use crate::resources::invoices::models::Invoice;

const INVOICES_PATH: &str = "/v1/invoices";
const INVOICES_BULK_PATH: &str = "/v1/invoices/bulk";
const MAX_BULK_INVOICES: usize = 50;

/// Request payload for creating a hosted e-invoice (`POST /v1/invoices`).
///
/// # Examples
///
/// ```
/// use some_moyasar::kernel::types::{Amount, Currency};
/// use some_moyasar::resources::invoices::create::CreateInvoiceParams;
///
/// let params = CreateInvoiceParams::new(Amount::from_minor(50_000), Currency::SAR)
///     .description("Enterprise annual subscription")
///     .callback_url("https://merchant.example.com/callbacks/moyasar");
///
/// assert_eq!(params.amount.minor_units(), 50_000);
/// assert_eq!(params.currency, Currency::SAR);
/// assert_eq!(
///     params.description.as_deref(),
///     Some("Enterprise annual subscription")
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateInvoiceParams {
    /// Invoice amount in the smallest currency unit (e.g., Halalas for SAR).
    pub amount: Amount,
    /// Three-letter ISO 4217 currency code.
    pub currency: Currency,
    /// Human-readable description displayed on the hosted invoice page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Webhook URL invoked when the invoice payment state changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_url: Option<String>,
    /// Redirect URL after a successful payment on the hosted page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success_url: Option<String>,
    /// Redirect URL when the customer returns without completing payment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub back_url: Option<String>,
    /// Optional UTC expiration timestamp for the hosted invoice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expired_at: Option<DateTime<Utc>>,
    /// Optional validated key/value metadata attached to the invoice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

impl CreateInvoiceParams {
    /// Creates a new [`CreateInvoiceParams`] with the required `amount` and `currency`.
    #[must_use]
    pub const fn new(amount: Amount, currency: Currency) -> Self {
        Self {
            amount,
            currency,
            description: None,
            callback_url: None,
            success_url: None,
            back_url: None,
            expired_at: None,
            metadata: None,
        }
    }

    /// Sets the invoice description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the callback URL for asynchronous payment notifications.
    #[must_use]
    pub fn callback_url(mut self, callback_url: impl Into<String>) -> Self {
        self.callback_url = Some(callback_url.into());
        self
    }

    /// Sets the redirect URL after successful payment.
    #[must_use]
    pub fn success_url(mut self, success_url: impl Into<String>) -> Self {
        self.success_url = Some(success_url.into());
        self
    }

    /// Sets the redirect URL when navigating back from the hosted page.
    #[must_use]
    pub fn back_url(mut self, back_url: impl Into<String>) -> Self {
        self.back_url = Some(back_url.into());
        self
    }

    /// Sets the UTC expiration timestamp for the invoice.
    #[must_use]
    pub const fn expired_at(mut self, expired_at: DateTime<Utc>) -> Self {
        self.expired_at = Some(expired_at);
        self
    }

    /// Attaches validated key/value metadata to the invoice.
    #[must_use]
    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

#[derive(Debug, Serialize)]
struct BulkCreateInvoicesRequest {
    invoices: Vec<CreateInvoiceParams>,
}

impl<'a, K: SecretAuthKey> InvoicesService<'a, K> {
    /// Creates a single hosted e-invoice (`POST /v1/invoices`).
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
    /// ```compile_fail
    /// use some_moyasar::kernel::auth::PublishableKey;
    /// use some_moyasar::kernel::client::MoyasarClient;
    /// use some_moyasar::kernel::types::{Amount, Currency};
    /// use some_moyasar::resources::invoices::create::CreateInvoiceParams;
    ///
    /// async fn reject_publishable(client: &MoyasarClient<PublishableKey>) {
    ///     let params = CreateInvoiceParams::new(Amount::from_minor(50_000), Currency::SAR);
    ///     let _ = client.invoices().create(&params).await;
    /// }
    /// ```
    pub async fn create(&self, params: &CreateInvoiceParams) -> Result<Invoice, MoyasarError> {
        let client = self.client();
        post_with_base_url(client, &client.config().base_url, INVOICES_PATH, params).await
    }

    /// Creates multiple hosted e-invoices in a single batch (`POST /v1/invoices/bulk`).
    ///
    /// Moves `invoices` directly into the `{"invoices": [...]}` request payload without
    /// cloning elements.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::Api`] on 4xx API rejections, [`MoyasarError::ServerError`]
    /// on unrecoverable 5xx responses, or [`MoyasarError::Network`] / [`MoyasarError::Timeout`]
    /// when transport retries are exhausted.
    ///
    /// # Examples
    ///
    /// ```compile_fail
    /// use some_moyasar::kernel::auth::PublishableKey;
    /// use some_moyasar::kernel::client::MoyasarClient;
    /// use some_moyasar::kernel::types::{Amount, Currency};
    /// use some_moyasar::resources::invoices::create::CreateInvoiceParams;
    ///
    /// async fn reject_publishable_bulk(client: &MoyasarClient<PublishableKey>) {
    ///     let batch = vec![CreateInvoiceParams::new(Amount::from_minor(10_000), Currency::SAR)];
    ///     let _ = client.invoices().create_bulk(batch).await;
    /// }
    /// ```
    pub async fn create_bulk(
        &self,
        invoices: Vec<CreateInvoiceParams>,
    ) -> Result<Vec<Invoice>, MoyasarError> {
        if invoices.is_empty() || invoices.len() > MAX_BULK_INVOICES {
            return Err(MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                message: format!(
                    "bulk invoice batch size must be between 1 and {MAX_BULK_INVOICES}, got {}",
                    invoices.len()
                ),
                errors: Vec::new(),
                response_code: None,
            });
        }
        let client = self.client();
        let payload = BulkCreateInvoicesRequest { invoices };
        post_with_base_url(
            client,
            &client.config().base_url,
            INVOICES_BULK_PATH,
            &payload,
        )
        .await
    }
}

async fn post_with_base_url<K, B, R>(
    client: &MoyasarClient<K>,
    base_url: &str,
    path: &str,
    body: &B,
) -> Result<R, MoyasarError>
where
    K: SecretAuthKey,
    B: Serialize + Sync,
    R: DeserializeOwned,
{
    let url = format!("{}{}", base_url.trim_end_matches('/'), path);
    execute_with_retry(client.config().max_network_retries, || {
        send_post(client, &url, body)
    })
    .await
}

async fn send_post<K, B, R>(
    client: &MoyasarClient<K>,
    url: &str,
    body: &B,
) -> Result<R, MoyasarError>
where
    K: SecretAuthKey,
    B: Serialize + Sync,
    R: DeserializeOwned,
{
    let response = client
        .http()
        .post(url)
        .json(body)
        .send()
        .await
        .map_err(MoyasarError::from)?;
    let status = response.status();
    let text = response.text().await.map_err(MoyasarError::from)?;
    if !status.is_success() {
        return Err(MoyasarError::from_api_response(status.as_u16(), &text));
    }
    serde_json::from_str::<R>(&text).map_err(|err| MoyasarError::Api {
        error_type: ApiErrorType::Unknown,
        message: format!("failed to decode invoice response: {err}"),
        errors: Vec::new(),
        response_code: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::SecretKey;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sample_invoice_json(id: &str, amount: i64, description: &str) -> serde_json::Value {
        json!({
            "id": id,
            "status": "created",
            "amount": amount,
            "currency": "SAR",
            "description": description,
            "logo_url": null,
            "amount_format": "500.00 SAR",
            "url": format!("https://moyasar.com/i/{id}"),
            "callback_url": "https://merchant.example.com/callbacks/moyasar",
            "expired_at": "2026-12-31T23:59:59Z",
            "back_url": null,
            "success_url": null,
            "metadata": {"order_id": "1001"},
            "payments": [],
            "created_at": "2026-11-13T10:00:00Z",
            "updated_at": "2026-11-13T10:00:00Z"
        })
    }

    #[test]
    fn create_invoice_params_omits_unset_optional_fields() {
        let params = CreateInvoiceParams::new(Amount::from_minor(50_000), Currency::SAR);
        let serialized = serde_json::to_value(&params).expect("serializes params");
        assert_eq!(serialized, json!({"amount": 50_000, "currency": "SAR"}));
    }

    #[tokio::test]
    async fn ac_us_4_2_0_01_tc_4_2_0_01_creates_hosted_invoice_with_url() {
        let server = MockServer::start().await;
        let expected_body = json!({
            "amount": 50_000,
            "currency": "SAR",
            "description": "Order #1001"
        });
        let response_json = sample_invoice_json(
            "11111111-1111-4111-8111-111111111111",
            50_000,
            "Order #1001",
        );

        Mock::given(method("POST"))
            .and(path("/v1/invoices"))
            .and(wiremock::matchers::body_json(&expected_body))
            .respond_with(ResponseTemplate::new(201).set_body_json(&response_json))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_invoice_create").expect("valid secret key");
        let client = MoyasarClient::new(key).expect("builds client");
        let params = CreateInvoiceParams::new(Amount::from_minor(50_000), Currency::SAR)
            .description("Order #1001");

        let invoice: Invoice = post_with_base_url(&client, &server.uri(), INVOICES_PATH, &params)
            .await
            .expect("creates invoice");

        assert!(invoice.url.as_deref().is_some_and(|u| !u.is_empty()));
        assert_eq!(
            invoice.url.as_deref(),
            Some("https://moyasar.com/i/11111111-1111-4111-8111-111111111111")
        );
    }

    #[tokio::test]
    async fn ac_us_4_2_0_02_tc_4_2_0_02_creates_bulk_invoices_with_three_items() {
        let server = MockServer::start().await;
        let batch = vec![
            CreateInvoiceParams::new(Amount::from_minor(10_000), Currency::SAR)
                .description("Bulk #1"),
            CreateInvoiceParams::new(Amount::from_minor(20_000), Currency::SAR)
                .description("Bulk #2"),
            CreateInvoiceParams::new(Amount::from_minor(30_000), Currency::SAR)
                .description("Bulk #3"),
        ];
        let expected_body = json!({
            "invoices": [
                {"amount": 10_000, "currency": "SAR", "description": "Bulk #1"},
                {"amount": 20_000, "currency": "SAR", "description": "Bulk #2"},
                {"amount": 30_000, "currency": "SAR", "description": "Bulk #3"}
            ]
        });
        let response_json = json!([
            sample_invoice_json("22222222-2222-4222-8222-222222222221", 10_000, "Bulk #1"),
            sample_invoice_json("22222222-2222-4222-8222-222222222222", 20_000, "Bulk #2"),
            sample_invoice_json("22222222-2222-4222-8222-222222222223", 30_000, "Bulk #3")
        ]);

        Mock::given(method("POST"))
            .and(path("/v1/invoices/bulk"))
            .and(wiremock::matchers::body_json(&expected_body))
            .respond_with(ResponseTemplate::new(201).set_body_json(&response_json))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_invoice_bulk").expect("valid secret key");
        let client = MoyasarClient::new(key).expect("builds client");
        let payload = BulkCreateInvoicesRequest { invoices: batch };

        let invoices: Vec<Invoice> =
            post_with_base_url(&client, &server.uri(), INVOICES_BULK_PATH, &payload)
                .await
                .expect("creates bulk invoices");

        assert_eq!(invoices.len(), 3);
        assert!(
            invoices
                .iter()
                .all(|inv| inv.url.as_deref().is_some_and(|u| !u.is_empty()))
        );
    }

    #[tokio::test]
    async fn create_invoice_maps_422_api_error_without_retrying() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/invoices"))
            .respond_with(ResponseTemplate::new(422).set_body_json(json!({
                "type": "invalid_request_error",
                "message": "Validation failed",
                "errors": {"amount": ["must be greater than 0"]}
            })))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_invoice_422").expect("valid secret key");
        let client = MoyasarClient::new(key).expect("builds client");
        let params = CreateInvoiceParams::new(Amount::from_minor(0), Currency::SAR);

        let err =
            post_with_base_url::<_, _, Invoice>(&client, &server.uri(), INVOICES_PATH, &params)
                .await
                .expect_err("422 must fail");

        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn create_invoice_retries_transient_503_and_recovers() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/invoices"))
            .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;

        let response_json =
            sample_invoice_json("33333333-3333-4333-8333-333333333333", 50_000, "Recovered");
        Mock::given(method("POST"))
            .and(path("/v1/invoices"))
            .respond_with(ResponseTemplate::new(201).set_body_json(&response_json))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_invoice_retry").expect("valid secret key");
        let client = MoyasarClient::new(key).expect("builds client");
        let params = CreateInvoiceParams::new(Amount::from_minor(50_000), Currency::SAR)
            .description("Recovered");

        let invoice: Invoice = post_with_base_url(&client, &server.uri(), INVOICES_PATH, &params)
            .await
            .expect("recovers on retry");

        assert_eq!(invoice.description.as_deref(), Some("Recovered"));
    }
}
