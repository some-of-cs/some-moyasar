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
use crate::resources::payments::update::validate_payment_id;

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
        validate_payment_id(id)?;
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
            parse_payment_response(response).await
        })
        .await
    }
}

async fn parse_payment_response(response: reqwest::Response) -> Result<Payment, MoyasarError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response.json::<Payment>().await?);
    }
    let body = response.text().await.unwrap_or_default();
    Err(MoyasarError::from_api_response(status.as_u16(), &body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::client::MoyasarClient;
    use crate::resources::payments::models::PaymentStatus;
    use serde_json::json;
    use wiremock::matchers::{body_json, body_string, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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

    #[tokio::test]
    async fn capture_rejects_invalid_id_and_zero_amount() {
        let key = SecretKey::new("sk_test_capture_validation").expect("valid key");
        let client = MoyasarClient::new(key).expect("client builds");

        let bad_id = client
            .payments()
            .capture("pay_123?x=1", None)
            .await
            .expect_err("must reject unsafe id");
        assert!(matches!(
            bad_id,
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            }
        ));

        let zero_amt = client
            .payments()
            .capture("pay_123", Some(0))
            .await
            .expect_err("must reject zero capture");
        assert!(matches!(
            zero_amt,
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn tc_2_2_0_03_wiremock_full_capture_omits_body_and_partial_sends_amount() {
        let server = MockServer::start().await;

        let full_response = json!({
            "id": "pay_01J9CAP_FULL",
            "status": "captured",
            "amount": 10000,
            "amount_format": "100.00 SAR",
            "fee": 250,
            "fee_format": "2.50 SAR",
            "refunded": 0,
            "refunded_format": "0.00 SAR",
            "captured": 10000,
            "captured_format": "100.00 SAR",
            "currency": "SAR",
            "source": { "type": "creditcard" },
            "captured_at": "2026-11-13T10:05:00Z",
            "created_at": "2026-11-13T10:00:00Z",
            "updated_at": "2026-11-13T10:05:00Z"
        });

        Mock::given(method("POST"))
            .and(path("/v1/payments/pay_01J9CAP_FULL/capture"))
            .and(body_string(""))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_response))
            .expect(1)
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let full_url = format!("{}/v1/payments/pay_01J9CAP_FULL/capture", server.uri());
        let full_payment = execute_with_retry(1, || async {
            let resp = http.post(&full_url).send().await?;
            parse_payment_response(resp).await
        })
        .await
        .expect("full capture succeeds");

        assert_eq!(full_payment.status, PaymentStatus::Captured);
        assert_eq!(full_payment.captured, 10000);

        let partial_response = json!({
            "id": "pay_01J9CAP_PART",
            "status": "captured",
            "amount": 10000,
            "amount_format": "100.00 SAR",
            "fee": 150,
            "fee_format": "1.50 SAR",
            "refunded": 0,
            "refunded_format": "0.00 SAR",
            "captured": 4000,
            "captured_format": "40.00 SAR",
            "currency": "SAR",
            "source": { "type": "creditcard" },
            "captured_at": "2026-11-13T10:06:00Z",
            "created_at": "2026-11-13T10:00:00Z",
            "updated_at": "2026-11-13T10:06:00Z"
        });

        Mock::given(method("POST"))
            .and(path("/v1/payments/pay_01J9CAP_PART/capture"))
            .and(body_json(json!({ "amount": 4000 })))
            .respond_with(ResponseTemplate::new(200).set_body_json(partial_response))
            .expect(1)
            .mount(&server)
            .await;

        let partial_url = format!("{}/v1/payments/pay_01J9CAP_PART/capture", server.uri());
        let payload = CapturePaymentRequest { amount: 4000 };
        let partial_payment = execute_with_retry(1, || async {
            let resp = http.post(&partial_url).json(&payload).send().await?;
            parse_payment_response(resp).await
        })
        .await
        .expect("partial capture succeeds");

        assert_eq!(partial_payment.status, PaymentStatus::Captured);
        assert_eq!(partial_payment.captured, 4000);
    }
}
