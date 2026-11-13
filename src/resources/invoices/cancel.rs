//! Invoice cancellation (`PUT /v1/invoices/:id/cancel`) and SADAD bill expiry synchronization.

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::transport::execute_with_retry;
use crate::resources::invoices::InvoicesService;
use crate::resources::invoices::models::Invoice;

const INVOICES_PATH: &str = "/v1/invoices";
const CANCEL_SUFFIX: &str = "/cancel";

/// Maximum SADAD bill validity window in days (`29` days).
pub const MAX_SADAD_EXPIRY_DAYS: i64 = 29;

/// Synchronizes a SADAD bill expiration timestamp with an invoice's `expired_at`.
///
/// Enforces the SAMA / Moyasar expiry-sync invariant:
/// `min(invoice_expired_at, now + 29 days)`. When `invoice_expired_at` is `None`,
/// the returned timestamp defaults to `now + 29 days`.
///
/// # Examples
///
/// ```
/// use chrono::{Duration, TimeZone, Utc};
/// use some_moyasar::resources::invoices::cancel::sync_sadad_expiry;
///
/// let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
/// let forty_days = Some(now + Duration::days(40));
/// assert_eq!(sync_sadad_expiry(forty_days, now), now + Duration::days(29));
///
/// let ten_days = Some(now + Duration::days(10));
/// assert_eq!(sync_sadad_expiry(ten_days, now), now + Duration::days(10));
/// ```
#[must_use]
pub fn sync_sadad_expiry(
    invoice_expired_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> DateTime<Utc> {
    let cap = now + Duration::days(MAX_SADAD_EXPIRY_DAYS);
    match invoice_expired_at {
        Some(expiry) => expiry.min(cap),
        None => cap,
    }
}

impl<'a, K: SecretAuthKey> InvoicesService<'a, K> {
    /// Cancels a hosted invoice via `PUT /v1/invoices/{id}/cancel`.
    ///
    /// Moyasar documents `PUT` (never `POST` or `DELETE`) for invoice cancellation,
    /// transitioning the invoice to the terminal `canceled` state. Requires a
    /// [`SecretAuthKey`] (`sk_live_...` or `sk_test_...`).
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::Api`] on 4xx responses (for example, `record_not_found`
    /// or `intent_not_cancelable`), [`MoyasarError::ServerError`] on 5xx responses,
    /// or [`MoyasarError::Network`] / [`MoyasarError::Timeout`] on transport failures.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    /// use uuid::Uuid;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let key = SecretKey::new("sk_test_abc123")?;
    /// let client = MoyasarClient::new(key)?;
    /// let invoice_id = Uuid::nil();
    /// let canceled = client.invoices().cancel(invoice_id).await?;
    /// println!("{:?}", canceled.status);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// A [`PublishableKey`](crate::kernel::auth::PublishableKey) cannot cancel invoices:
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use uuid::Uuid;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let pk = PublishableKey::new("pk_test_abc123")?;
    /// let client = MoyasarClient::new(pk)?;
    /// let _ = client.invoices().cancel(Uuid::nil()).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn cancel(&self, id: Uuid) -> Result<Invoice, MoyasarError> {
        self.cancel_with_base_url(self.client().config().base_url.as_str(), id)
            .await
    }

    async fn cancel_with_base_url(
        &self,
        base_url: &str,
        id: Uuid,
    ) -> Result<Invoice, MoyasarError> {
        let base = base_url.trim_end_matches('/');
        let url = format!("{base}{INVOICES_PATH}/{id}{CANCEL_SUFFIX}");
        execute_cancel_request(
            self.client().http(),
            self.client().config().max_network_retries,
            &url,
        )
        .await
    }
}

async fn execute_cancel_request(
    http: &reqwest::Client,
    max_network_retries: u32,
    url: &str,
) -> Result<Invoice, MoyasarError> {
    execute_with_retry(max_network_retries, || send_cancel_once(http, url)).await
}

async fn send_cancel_once(http: &reqwest::Client, url: &str) -> Result<Invoice, MoyasarError> {
    let response = http.put(url).send().await.map_err(map_reqwest_error)?;
    let status = response.status();
    let body = response.text().await.map_err(map_reqwest_error)?;

    if !status.is_success() {
        return Err(MoyasarError::from_api_response(status.as_u16(), &body));
    }

    serde_json::from_str::<Invoice>(&body).map_err(|err| MoyasarError::Api {
        error_type: ApiErrorType::Unknown,
        message: format!("failed to decode invoice response: {err}"),
        errors: Vec::new(),
        response_code: None,
    })
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
    use crate::resources::invoices::models::InvoiceStatus;
    use chrono::TimeZone;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn tc_4_2_0_05_cancel_uses_put_verb_on_cancel_path() {
        let server = MockServer::start().await;
        let invoice_id = Uuid::new_v4();
        let expected_path = format!("/v1/invoices/{invoice_id}/cancel");

        let body = json!({
            "id": invoice_id,
            "status": "canceled",
            "amount": 50000,
            "currency": "SAR",
            "description": "Hosted e-invoice",
            "url": format!("https://moyasar.com/invoices/{invoice_id}"),
            "expired_at": "2026-12-01T00:00:00Z",
            "metadata": {},
            "payments": [],
            "created_at": "2026-11-13T00:00:00Z"
        });

        Mock::given(method("PUT"))
            .and(path(expected_path.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_cancel_unit_5").expect("valid secret key");
        let client = MoyasarClient::new(key).expect("client builds");

        let invoice = client
            .invoices()
            .cancel_with_base_url(&server.uri(), invoice_id)
            .await
            .expect("cancel succeeds via PUT");

        assert_eq!(invoice.id, invoice_id);
        assert_eq!(invoice.status, InvoiceStatus::Canceled);
    }

    #[tokio::test]
    async fn cancel_does_not_retry_4xx_and_retries_5xx() {
        let server = MockServer::start().await;
        let invoice_id = Uuid::new_v4();
        let expected_path = format!("/v1/invoices/{invoice_id}/cancel");

        Mock::given(method("PUT"))
            .and(path(expected_path.as_str()))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "type": "intent_not_cancelable",
                "message": "Invoice already paid"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_cancel_err").expect("valid secret key");
        let client = MoyasarClient::builder(key)
            .max_network_retries(2)
            .build()
            .expect("client builds");

        let err = client
            .invoices()
            .cancel_with_base_url(&server.uri(), invoice_id)
            .await
            .expect_err("400 must fail without retry");

        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::IntentNotCancelable,
                ..
            }
        ));
    }

    #[test]
    fn tc_4_2_0_06_sync_sadad_expiry_caps_at_29_days_and_respects_earlier_invoice_expiry() {
        let now = Utc.with_ymd_and_hms(2026, 11, 13, 12, 0, 0).unwrap();
        let max_cap = now + Duration::days(29);

        let forty_days_out = Some(now + Duration::days(40));
        assert_eq!(sync_sadad_expiry(forty_days_out, now), max_cap);

        let exact_cap = Some(now + Duration::days(29));
        assert_eq!(sync_sadad_expiry(exact_cap, now), max_cap);

        let ten_days_out = Some(now + Duration::days(10));
        assert_eq!(
            sync_sadad_expiry(ten_days_out, now),
            now + Duration::days(10)
        );

        let past_expiry = Some(now - Duration::days(1));
        assert_eq!(sync_sadad_expiry(past_expiry, now), now - Duration::days(1));

        assert_eq!(sync_sadad_expiry(None, now), max_cap);
    }
}
