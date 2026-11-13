//! Invoice update (`UpdateInvoiceParams`, `InvoicesService::update`).
//!
//! Enforces SAMA e-invoicing immutability (`TC-4.2.0-LAW`): `amount` and `currency`
//! cannot be modified after invoice issuance; only `description` and `metadata`
//! are mutable via `PUT /v1/invoices/{id}`.

use serde::Serialize;
use uuid::Uuid;

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::metadata::Metadata;
use crate::kernel::transport::send_with_retry;
use crate::resources::invoices::InvoicesService;
use crate::resources::invoices::models::Invoice;

/// Request payload for updating an existing Moyasar hosted invoice (`PUT /v1/invoices/{id}`).
///
/// By design, `UpdateInvoiceParams` exposes only `description` and `metadata` so
/// `amount` and `currency` remain immutable across the invoice lifecycle (`TC-4.2.0-LAW`).
///
/// # Examples
///
/// ```
/// use some_moyasar::prelude::Metadata;
/// use some_moyasar::resources::invoices::update::UpdateInvoiceParams;
///
/// let mut metadata = Metadata::new();
/// metadata.insert("order_id", "ORD-420").unwrap();
///
/// let params = UpdateInvoiceParams::new()
///     .description("Updated Q4 retainer invoice")
///     .metadata(metadata);
///
/// assert_eq!(
///     params.description.as_deref(),
///     Some("Updated Q4 retainer invoice")
/// );
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UpdateInvoiceParams {
    /// Updated human-readable description for the hosted invoice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Updated key-value metadata map (up to 30 keys, 40-char keys, 500-char values).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

impl UpdateInvoiceParams {
    /// Creates an empty [`UpdateInvoiceParams`] payload.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the updated invoice description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the updated invoice metadata map.
    #[must_use]
    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

impl<'a, K: SecretAuthKey> InvoicesService<'a, K> {
    /// Updates an existing invoice's `description` and/or `metadata` via `PUT /v1/invoices/{id}`.
    ///
    /// Requires a backend [`SecretAuthKey`] (`sk_live_...` or `sk_test_...`). `amount` and
    /// `currency` are immutable and cannot be modified through this endpoint.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{Metadata, MoyasarClient, SecretKey};
    /// use some_moyasar::resources::invoices::update::UpdateInvoiceParams;
    /// use uuid::Uuid;
    ///
    /// # async fn run() -> Result<(), some_moyasar::prelude::MoyasarError> {
    /// let key = SecretKey::new("sk_test_example")?;
    /// let client = MoyasarClient::new(key)?;
    /// let mut metadata = Metadata::new();
    /// metadata.insert("order_id", "ORD-420")?;
    ///
    /// let params = UpdateInvoiceParams::new()
    ///     .description("Updated invoice description")
    ///     .metadata(metadata);
    ///
    /// let invoice = client.invoices().update(Uuid::nil(), &params).await?;
    /// println!("Updated invoice: {}", invoice.id);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Calling `update` with a [`PublishableKey`](crate::kernel::auth::PublishableKey)
    /// fails to compile because [`SecretAuthKey`] is required:
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::invoices::update::UpdateInvoiceParams;
    /// use uuid::Uuid;
    ///
    /// # async fn demo() -> Result<(), some_moyasar::prelude::MoyasarError> {
    /// let pk = PublishableKey::new("pk_test_abc123")?;
    /// let client = MoyasarClient::new(pk)?;
    /// let params = UpdateInvoiceParams::new().description("Should not compile");
    /// let _ = client.invoices().update(Uuid::nil(), &params).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn update(
        &self,
        id: Uuid,
        params: &UpdateInvoiceParams,
    ) -> Result<Invoice, MoyasarError> {
        send_update_request(
            self.client().http(),
            &self.client().config().base_url,
            self.client().config().max_network_retries,
            id,
            params,
        )
        .await
    }
}

async fn send_update_request(
    http: &reqwest::Client,
    base_url: &str,
    max_network_retries: u32,
    id: Uuid,
    params: &UpdateInvoiceParams,
) -> Result<Invoice, MoyasarError> {
    let url = format!("{}/v1/invoices/{id}", base_url.trim_end_matches('/'));
    send_with_retry(max_network_retries, || async {
        let response = http
            .put(&url)
            .json(params)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        let status = response.status();
        let body = response.text().await.map_err(map_reqwest_error)?;
        if !status.is_success() {
            return Err(MoyasarError::from_api_response(status.as_u16(), &body));
        }
        deserialize_invoice(&body)
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

fn deserialize_invoice(body: &str) -> Result<Invoice, MoyasarError> {
    serde_json::from_str::<Invoice>(body).map_err(|err| MoyasarError::Api {
        error_type: ApiErrorType::Unknown,
        message: format!("failed to deserialize invoice response: {err}"),
        errors: Vec::new(),
        response_code: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::transport::{API_VERSION, MOYASAR_VERSION_HEADER, build_default_headers};
    use serde_json::json;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_http_client() -> reqwest::Client {
        let key = SecretKey::new("sk_test_wiremock_unit4").expect("valid secret key");
        let headers = build_default_headers(&key).expect("valid default headers");
        reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .expect("reqwest client builds")
    }

    fn sample_invoice_json(id: Uuid, description: &str) -> serde_json::Value {
        json!({
            "id": id,
            "status": "created",
            "amount": 50000,
            "currency": "SAR",
            "description": description,
            "url": format!("https://moyasar.com/invoices/{id}"),
            "expired_at": "2026-12-01T00:00:00Z",
            "metadata": {
                "order_id": "ORD-420"
            },
            "payments": [],
            "created_at": "2026-11-13T10:00:00Z"
        })
    }

    #[test]
    fn tc_4_2_0_law_update_params_enforces_amount_and_currency_immutability() {
        let empty = UpdateInvoiceParams::new();
        let empty_json = serde_json::to_value(&empty).expect("serialize empty params");
        assert_eq!(empty_json, json!({}));

        let mut metadata = Metadata::new();
        metadata
            .insert("order_id", "ORD-420")
            .expect("valid metadata");
        let params = UpdateInvoiceParams::new()
            .description("Updated Q4 retainer")
            .metadata(metadata);

        let serialized = serde_json::to_value(&params).expect("serialize update params");
        let obj = serialized.as_object().expect("JSON object");
        assert_eq!(obj.len(), 2);
        assert_eq!(obj.get("description"), Some(&json!("Updated Q4 retainer")));
        assert_eq!(obj.get("metadata"), Some(&json!({ "order_id": "ORD-420" })));
        assert!(!obj.contains_key("amount"));
        assert!(!obj.contains_key("currency"));
    }

    #[tokio::test]
    async fn ac_us_4_2_0_04_tc_4_2_0_04_updates_description_and_metadata_via_put() {
        let server = MockServer::start().await;
        let invoice_id = Uuid::new_v4();
        let expected_path = format!("/v1/invoices/{invoice_id}");

        let mut metadata = Metadata::new();
        metadata
            .insert("order_id", "ORD-420")
            .expect("valid metadata");
        let params = UpdateInvoiceParams::new()
            .description("Updated Q4 retainer invoice")
            .metadata(metadata);

        Mock::given(method("PUT"))
            .and(path(expected_path.as_str()))
            .and(header(MOYASAR_VERSION_HEADER, API_VERSION))
            .and(body_json(json!({
                "description": "Updated Q4 retainer invoice",
                "metadata": { "order_id": "ORD-420" }
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(sample_invoice_json(
                    invoice_id,
                    "Updated Q4 retainer invoice",
                )),
            )
            .expect(1)
            .mount(&server)
            .await;

        let http = test_http_client();
        let updated = send_update_request(&http, &server.uri(), 2, invoice_id, &params)
            .await
            .expect("update succeeds");

        assert_eq!(updated.id, invoice_id);
        assert_eq!(
            updated.description.as_deref(),
            Some("Updated Q4 retainer invoice")
        );
    }

    #[tokio::test]
    async fn maps_422_validation_error_without_retrying() {
        let server = MockServer::start().await;
        let invoice_id = Uuid::new_v4();
        let expected_path = format!("/v1/invoices/{invoice_id}");

        Mock::given(method("PUT"))
            .and(path(expected_path.as_str()))
            .respond_with(ResponseTemplate::new(422).set_body_json(json!({
                "type": "invalid_request_error",
                "message": "Description is too long",
                "errors": { "description": ["is too long"] }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let http = test_http_client();
        let params = UpdateInvoiceParams::new().description("Too long");
        let err = send_update_request(&http, &server.uri(), 2, invoice_id, &params)
            .await
            .expect_err("422 returns Api error");

        assert!(!err.is_retryable());
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn retries_transient_503_before_succeeding() {
        let server = MockServer::start().await;
        let invoice_id = Uuid::new_v4();
        let expected_path = format!("/v1/invoices/{invoice_id}");

        Mock::given(method("PUT"))
            .and(path(expected_path.as_str()))
            .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("PUT"))
            .and(path(expected_path.as_str()))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(sample_invoice_json(invoice_id, "Recovered description")),
            )
            .expect(1)
            .mount(&server)
            .await;

        let http = test_http_client();
        let params = UpdateInvoiceParams::new().description("Recovered description");
        let updated = send_update_request(&http, &server.uri(), 2, invoice_id, &params)
            .await
            .expect("recovers after transient 503");

        assert_eq!(
            updated.description.as_deref(),
            Some("Recovered description")
        );
    }
}
