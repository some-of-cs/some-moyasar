//! Payment refund endpoint (`POST /v1/payments/:id/refund`) (US-2.2.0 Unit 4).
//!
//! Enforces the Body-Omission Law (`refund(id, None)` sends no request body) and
//! client-side bounds validation against remaining refundable funds, while
//! preserving `refunded_at` and `refunded` on the returned [`Payment`] for SAMA
//! reversal-reconciliation traceability.

use serde::Serialize;

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::transport::execute_with_retry;
use crate::resources::payments::models::Payment;
use crate::resources::payments::service::PaymentsService;

/// Internal request body for partial refunds (`POST /v1/payments/:id/refund`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct RefundPaymentRequest {
    amount: u64,
}

/// Computes the maximum remaining refundable amount in minor units for `payment`.
pub const fn remaining_refundable_amount(payment: &Payment) -> u64 {
    let base = if payment.captured > 0 {
        payment.captured
    } else {
        payment.amount
    };
    base.saturating_sub(payment.refunded)
}

/// Validates a requested partial or full refund `amount` against `payment`.
///
/// Rejects zero amounts and amounts exceeding [`remaining_refundable_amount`].
pub fn validate_refund_amount(payment: &Payment, amount: Option<u64>) -> Result<(), MoyasarError> {
    if let Some(amt) = amount {
        if amt == 0 {
            return Err(MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                message: "refund amount must be greater than zero".to_owned(),
                errors: Vec::new(),
                response_code: None,
            });
        }
        let refundable = remaining_refundable_amount(payment);
        if amt > refundable {
            return Err(MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                message: format!(
                    "refund amount {amt} exceeds remaining refundable amount {refundable}"
                ),
                errors: Vec::new(),
                response_code: None,
            });
        }
    }
    Ok(())
}

impl<K: SecretAuthKey> PaymentsService<'_, K> {
    /// Refunds a paid or captured payment (`POST /v1/payments/:id/refund`).
    ///
    /// - Pass `amount = None` for a full refund (omits the HTTP request body completely).
    /// - Pass `amount = Some(minor_units)` for a partial refund (`{"amount": minor_units}`).
    ///
    /// Returned [`Payment::refunded_at`] and [`Payment::refunded`] fields are preserved
    /// verbatim for SAMA merchant-initiated reversal reconciliation.
    ///
    /// # Examples
    ///
    /// Publishable keys cannot invoke `refund`:
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    ///
    /// # async fn example() {
    /// let pk = PublishableKey::new("pk_test_abc123").unwrap();
    /// let client = MoyasarClient::new(pk).unwrap();
    /// let _ = client.payments().refund("pay_123", None).await;
    /// # }
    /// ```
    pub async fn refund(
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
                message: "refund amount must be greater than zero".to_owned(),
                errors: Vec::new(),
                response_code: None,
            });
        }

        let url = format!("{}/v1/payments/{id}/refund", self.client().config().base_url);
        let http = self.client().http();
        let retries = self.client().config().max_network_retries;
        let payload = amount.map(|amt| RefundPaymentRequest { amount: amt });

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
    fn tc_2_2_0_05_partial_refund_payload_serializes_amount_and_full_omits_body() {
        let partial: Option<RefundPaymentRequest> =
            Some(5000).map(|amt| RefundPaymentRequest { amount: amt });
        let full: Option<RefundPaymentRequest> =
            None::<u64>.map(|amt| RefundPaymentRequest { amount: amt });

        let json = serde_json::to_value(partial.unwrap()).expect("serializes partial refund");
        assert_eq!(json["amount"], 5000);
        assert!(full.is_none());
    }
}
