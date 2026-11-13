//! Payment void endpoint (`POST /v1/payments/:id/void`) and 2-hour unsettled window guard (US-2.2.0 Unit 3).

use chrono::{DateTime, Utc};

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::transport::execute_with_retry;
use crate::resources::payments::models::{Payment, PaymentStatus};
use crate::resources::payments::service::PaymentsService;

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
    match (&payment.status, unsettled) {
        (PaymentStatus::Authorized, _) => true,
        (PaymentStatus::Paid | PaymentStatus::Captured, true) => true,
        _ => false,
    }
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
        if id.trim().is_empty() {
            return Err(MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                message: "payment id must not be empty".to_owned(),
                errors: Vec::new(),
                response_code: None,
            });
        }

        let url = format!("{}/v1/payments/{id}/void", self.client().config().base_url);
        let http = self.client().http();
        let retries = self.client().config().max_network_retries;

        execute_with_retry(retries, || async {
            let response = http.post(&url).send().await?;
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
    use chrono::Duration;

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
}
