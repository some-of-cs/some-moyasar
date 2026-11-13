//! Payment void endpoint (`POST /v1/payments/:id/void`) and 2-hour unsettled window guard (US-2.2.0 Unit 3).

use chrono::{DateTime, Utc};

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::transport::execute_with_retry;
use crate::resources::payments::models::{Payment, PaymentStatus};
use crate::resources::payments::service::PaymentsService;
use crate::resources::payments::update::validate_payment_id;

/// Unsettled void window in seconds (2 hours = 7,200 seconds).
///
/// Moyasar permits voiding `paid` or `captured` payments only within ~2 hours
/// of creation before settlement; after that window, a `refund` is required.
pub const VOID_WINDOW_SECS: i64 = 7_200;

/// Returns `true` when `now` is strictly within the [`VOID_WINDOW_SECS`] (2-hour)
/// unsettled window relative to `created_at`.
pub fn is_within_void_window(created_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    (now - created_at).num_seconds() < VOID_WINDOW_SECS
}

/// Returns `true` if `payment` is eligible for `void()` at `now`.
///
/// - `Authorized` payments can be voided regardless of the 2-hour settlement window.
/// - `Paid` and `Captured` payments can be voided only while [`is_within_void_window`] is `true`.
pub fn can_void_payment(payment: &Payment, now: DateTime<Utc>) -> bool {
    let unsettled = is_within_void_window(payment.created_at, now);
    matches!(
        (&payment.status, unsettled),
        (PaymentStatus::Authorized, _) | (PaymentStatus::Paid | PaymentStatus::Captured, true)
    )
}

impl<K: SecretAuthKey> PaymentsService<'_, K> {
    /// Voids an unsettled or authorized payment (`POST /v1/payments/:id/void`).
    ///
    /// Sends an empty request body. Requires a [`SecretAuthKey`];
    /// [`PublishableKey`](crate::kernel::auth::PublishableKey) is rejected at compile time.
    ///
    /// # Examples
    ///
    /// Publishable keys cannot invoke `void`:
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    ///
    /// # async fn example() {
    /// let pk = PublishableKey::new("pk_test_abc123").unwrap();
    /// let client = MoyasarClient::new(pk).unwrap();
    /// let _ = client.payments().void("pay_123").await;
    /// # }
    /// ```
    pub async fn void(&self, id: &str) -> Result<Payment, MoyasarError> {
        validate_payment_id(id)?;

        let url = format!(
            "{}/v1/payments/{id}/void",
            self.client().config().base_url.trim_end_matches('/')
        );
        let http = self.client().http();
        let retries = self.client().config().max_network_retries;

        execute_with_retry(retries, || async {
            let response = http.post(&url).send().await?;
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
    use crate::kernel::error::ApiErrorType;
    use crate::kernel::metadata::Metadata;
    use crate::kernel::types::Currency;
    use chrono::Duration;
    use serde_json::json;
    use wiremock::matchers::{body_string, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sample_payment(status: PaymentStatus, created_at: DateTime<Utc>) -> Payment {
        Payment {
            id: "pay_01J9VOID".to_owned(),
            status,
            amount: 15000,
            amount_format: "150.00 SAR".to_owned(),
            fee: 350,
            fee_format: "3.50 SAR".to_owned(),
            refunded: 0,
            refunded_format: "0.00 SAR".to_owned(),
            captured: 15000,
            captured_format: "150.00 SAR".to_owned(),
            currency: Currency::SAR,
            description: None,
            source: json!({ "type": "creditcard" }),
            metadata: Metadata::default(),
            splits: None,
            callback_url: None,
            invoice_id: None,
            ip: None,
            captured_at: None,
            refunded_at: None,
            voided_at: None,
            created_at,
            updated_at: created_at,
        }
    }

    #[test]
    fn tc_2_2_0_04_void_window_boundary_is_two_hours() {
        let now = Utc::now();
        let within = now - Duration::minutes(30);
        let at_boundary = now - Duration::seconds(VOID_WINDOW_SECS);
        let past = now - Duration::hours(5);

        assert!(is_within_void_window(within, now));
        assert!(!is_within_void_window(at_boundary, now));
        assert!(!is_within_void_window(past, now));
    }

    #[test]
    fn can_void_payment_respects_status_and_two_hour_window() {
        let now = Utc::now();
        let within = now - Duration::minutes(30);
        let at_boundary = now - Duration::seconds(VOID_WINDOW_SECS);
        let settled = now - Duration::hours(5);

        assert!(can_void_payment(
            &sample_payment(PaymentStatus::Authorized, within),
            now
        ));
        assert!(can_void_payment(
            &sample_payment(PaymentStatus::Authorized, settled),
            now
        ));

        for status in [PaymentStatus::Paid, PaymentStatus::Captured] {
            assert!(can_void_payment(
                &sample_payment(status.clone(), within),
                now
            ));
            assert!(!can_void_payment(
                &sample_payment(status.clone(), at_boundary),
                now
            ));
            assert!(!can_void_payment(&sample_payment(status, settled), now));
        }

        for terminal in [
            PaymentStatus::Voided,
            PaymentStatus::Refunded,
            PaymentStatus::Failed,
            PaymentStatus::Initiated,
        ] {
            assert!(!can_void_payment(&sample_payment(terminal, within), now));
        }
    }

    #[tokio::test]
    async fn void_rejects_invalid_path_segments() {
        let key = SecretKey::new("sk_test_void_validation").expect("valid key");
        let client = MoyasarClient::new(key).expect("client builds");

        for bad in ["", "   ", "pay_1/refund", "pay_1?a=1", "pay_1#frag", "../x"] {
            let err = client
                .payments()
                .void(bad)
                .await
                .expect_err("must reject unsafe id");
            assert!(matches!(
                err,
                MoyasarError::Api {
                    error_type: ApiErrorType::InvalidRequestError,
                    ..
                }
            ));
        }
    }

    #[tokio::test]
    async fn tc_2_2_0_04_wiremock_void_sends_empty_post_and_returns_voided_payment() {
        let server = MockServer::start().await;

        let response_body = json!({
            "id": "pay_01J9VOID",
            "status": "voided",
            "amount": 15000,
            "amount_format": "150.00 SAR",
            "fee": 0,
            "fee_format": "0.00 SAR",
            "refunded": 0,
            "refunded_format": "0.00 SAR",
            "captured": 0,
            "captured_format": "0.00 SAR",
            "currency": "SAR",
            "source": { "type": "creditcard" },
            "voided_at": "2026-11-13T10:15:00Z",
            "created_at": "2026-11-13T10:00:00Z",
            "updated_at": "2026-11-13T10:15:00Z"
        });

        Mock::given(method("POST"))
            .and(path("/v1/payments/pay_01J9VOID/void"))
            .and(body_string(""))
            .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
            .expect(1)
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let url = format!("{}/v1/payments/pay_01J9VOID/void", server.uri());
        let payment = execute_with_retry(1, || async {
            let resp = http.post(&url).send().await?;
            parse_payment_response(resp).await
        })
        .await
        .expect("void succeeds");

        assert_eq!(payment.id, "pay_01J9VOID");
        assert_eq!(payment.status, PaymentStatus::Voided);
        assert!(payment.voided_at.is_some());
    }
}
