//! Payment capture endpoint (`POST /v1/payments/:id/capture`) (US-2.2.0 Unit 2).
//!
//! Supports full (`None` — empty request body) and partial (`Some(amount)`) capture
//! of authorized payment holds, plus Mada 14-day capture-window awareness.

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::transport::execute_with_retry;
use crate::resources::payments::models::Payment;
use crate::resources::payments::service::PaymentsService;

/// Maximum Mada authorization hold window in days before issuers silently release funds.
pub const MADA_CAPTURE_WINDOW_DAYS: i64 = 14;

/// Internal request body for partial capture (`POST /v1/payments/:id/capture`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct CapturePaymentRequest {
    amount: u64,
}

/// Returns `true` if an authorized Mada payment is older than [`MADA_CAPTURE_WINDOW_DAYS`] (14 days),
/// indicating the issuer hold has likely expired even if `status` still reads `authorized`.
pub fn is_mada_hold_likely_expired(created_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    (now - created_at) > Duration::days(MADA_CAPTURE_WINDOW_DAYS)
}

impl<K: SecretAuthKey> PaymentsService<'_, K> {
    /// Captures an authorized payment hold (`POST /v1/payments/:id/capture`).
    ///
    /// - Pass `amount = None` to capture the full authorized amount (sends an empty body).
    /// - Pass `amount = Some(minor_units)` to perform a partial capture (`{"amount": minor_units}`).
    ///
    /// # Mada 14-Day Capture Window
    ///
    /// Mada issuer holds silently expire after 14 days while Moyasar's payment `status`
    /// may still report `authorized`. Call [`is_mada_hold_likely_expired`] before assuming
    /// an older authorized Mada hold is still capturable or settled.
    ///
    /// # Examples
    ///
    /// Publishable keys cannot invoke `capture`:
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    ///
    /// # async fn example() {
    /// let pk = PublishableKey::new("pk_test_abc123").unwrap();
    /// let client = MoyasarClient::new(pk).unwrap();
    /// let _ = client.payments().capture("pay_123", Some(4000)).await;
    /// # }
    /// ```
    pub async fn capture(
        &self,
        id: &str,
        amount: Option<u64>,
    ) -> Result<Payment, MoyasarError> {
        if id.trim().is_empty() {
            return Err(MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                message: "payment id must not be empty".to_owned(),
                errors: Vec::new(),
                response_code: None,
            });
        }
        if let Some(0) = amount {
            return Err(MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                message: "capture amount must be greater than zero".to_owned(),
                errors: Vec::new(),
                response_code: None,
            });
        }

        let url = format!("{}/v1/payments/{id}/capture", self.client().config().base_url);
        let http = self.client().http();
        let retries = self.client().config().max_network_retries;
        let payload = amount.map(|amt| CapturePaymentRequest { amount: amt });

        execute_with_retry(retries, || async {
            let req = match payload {
                Some(ref body) => http.post(&url).json(body),
                None => http.post(&url),
            };
            let response = req.send().await?;
            let status = response.status();
            if status.is_success() {
                return Ok(response.json::<Payment>().await?);
            }
            let body = response.text().await.unwrap_or_default();
            Err(MoyasarError::from_api_response(status.as_u16(), &body))
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_2_2_0_03_partial_capture_payload_serializes_amount() {
        let req = CapturePaymentRequest { amount: 4000 };
        let json = serde_json::to_value(req).expect("serializes capture payload");
        assert_eq!(json["amount"], 4000);
    }

    #[test]
    fn tc_2_2_0_07_mada_14_day_hold_expiry_detection() {
        let now = Utc::now();
        let recent = now - Duration::days(13);
        let expired = now - Duration::days(15);
        assert!(!is_mada_hold_likely_expired(recent, now));
        assert!(is_mada_hold_likely_expired(expired, now));
    }
}
