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
use crate::resources::payments::update::{parse_payment_response, validate_payment_id};

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
    pub async fn refund(&self, id: &str, amount: Option<u64>) -> Result<Payment, MoyasarError> {
        validate_payment_id(id)?;
        if let Some(0) = amount {
            return Err(MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                message: "refund amount must be greater than zero".to_owned(),
                errors: Vec::new(),
                response_code: None,
            });
        }

        let url = format!(
            "{}/v1/payments/{id}/refund",
            self.client().config().base_url.trim_end_matches('/')
        );
        let http = self.client().http();
        let retries = self.client().config().max_network_retries;
        let payload = amount.map(|amt| RefundPaymentRequest { amount: amt });

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

    /// Validates `amount` against `payment`'s [`remaining_refundable_amount`] before
    /// issuing [`PaymentsService::refund`].
    ///
    /// Enforces Spec §5.4 client-side bounds validation when `amount` is `Some(_)`.
    pub async fn refund_payment(
        &self,
        payment: &Payment,
        amount: Option<u64>,
    ) -> Result<Payment, MoyasarError> {
        validate_refund_amount(payment, amount)?;
        self.refund(&payment.id, amount).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::client::MoyasarClient;
    use crate::kernel::metadata::Metadata;
    use crate::kernel::types::Currency;
    use crate::resources::payments::models::PaymentStatus;
    use chrono::Utc;
    use serde_json::json;
    use wiremock::matchers::{body_json, body_string, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sample_payment(amount: u64, captured: u64, refunded: u64) -> Payment {
        let now = Utc::now();
        Payment {
            id: "pay_01J9REF".to_owned(),
            status: PaymentStatus::Paid,
            amount,
            amount_format: "150.00 SAR".to_owned(),
            fee: 350,
            fee_format: "3.50 SAR".to_owned(),
            refunded,
            refunded_format: "0.00 SAR".to_owned(),
            captured,
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
            created_at: now,
            updated_at: now,
        }
    }

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

    #[test]
    fn remaining_refundable_amount_uses_captured_or_amount_and_subtracts_refunded() {
        let uncaptured_paid = sample_payment(15000, 0, 5000);
        assert_eq!(remaining_refundable_amount(&uncaptured_paid), 10000);

        let partially_captured = sample_payment(15000, 10000, 3000);
        assert_eq!(remaining_refundable_amount(&partially_captured), 7000);

        let over_refunded = sample_payment(10000, 10000, 12000);
        assert_eq!(remaining_refundable_amount(&over_refunded), 0);
    }

    #[test]
    fn validate_refund_amount_enforces_spec_5_4_bounds() {
        let payment = sample_payment(15000, 10000, 2500);
        assert!(validate_refund_amount(&payment, None).is_ok());
        assert!(validate_refund_amount(&payment, Some(1)).is_ok());
        assert!(validate_refund_amount(&payment, Some(7500)).is_ok());

        let zero_err =
            validate_refund_amount(&payment, Some(0)).expect_err("must reject zero refund");
        assert!(matches!(
            zero_err,
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            }
        ));

        let over_err =
            validate_refund_amount(&payment, Some(7501)).expect_err("must reject over-refund");
        assert!(matches!(
            over_err,
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn refund_and_refund_payment_reject_invalid_id_and_over_refund() {
        let key = SecretKey::new("sk_test_refund_validation").expect("valid key");
        let client = MoyasarClient::new(key).expect("client builds");

        let bad_id = client
            .payments()
            .refund("pay_123/void", None)
            .await
            .expect_err("must reject unsafe id");
        assert!(matches!(
            bad_id,
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            }
        ));

        let payment = sample_payment(15000, 10000, 6000);
        let over_refund = client
            .payments()
            .refund_payment(&payment, Some(4001))
            .await
            .expect_err("must reject refund exceeding remaining refundable amount");
        assert!(matches!(
            over_refund,
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn tc_2_2_0_05_and_law_wiremock_full_vs_partial_refund_and_refunded_at_traceability() {
        let server = MockServer::start().await;

        let full_response = json!({
            "id": "pay_01J9REF_FULL",
            "status": "refunded",
            "amount": 15000,
            "amount_format": "150.00 SAR",
            "fee": 350,
            "fee_format": "3.50 SAR",
            "refunded": 15000,
            "refunded_format": "150.00 SAR",
            "captured": 15000,
            "captured_format": "150.00 SAR",
            "currency": "SAR",
            "source": { "type": "creditcard" },
            "refunded_at": "2026-11-13T12:00:00Z",
            "created_at": "2026-11-13T08:00:00Z",
            "updated_at": "2026-11-13T12:00:00Z"
        });

        Mock::given(method("POST"))
            .and(path("/v1/payments/pay_01J9REF_FULL/refund"))
            .and(body_string(""))
            .respond_with(ResponseTemplate::new(200).set_body_json(full_response))
            .expect(1)
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let full_url = format!("{}/v1/payments/pay_01J9REF_FULL/refund", server.uri());
        let full_payment = execute_with_retry(1, || async {
            let resp = http.post(&full_url).send().await?;
            parse_payment_response(resp).await
        })
        .await
        .expect("full refund succeeds");

        assert_eq!(full_payment.status, PaymentStatus::Refunded);
        assert_eq!(full_payment.refunded, 15000);
        assert!(full_payment.refunded_at.is_some());

        let partial_response = json!({
            "id": "pay_01J9REF_PART",
            "status": "paid",
            "amount": 15000,
            "amount_format": "150.00 SAR",
            "fee": 350,
            "fee_format": "3.50 SAR",
            "refunded": 5000,
            "refunded_format": "50.00 SAR",
            "captured": 15000,
            "captured_format": "150.00 SAR",
            "currency": "SAR",
            "source": { "type": "creditcard" },
            "refunded_at": "2026-11-13T12:05:00Z",
            "created_at": "2026-11-13T08:00:00Z",
            "updated_at": "2026-11-13T12:05:00Z"
        });

        Mock::given(method("POST"))
            .and(path("/v1/payments/pay_01J9REF_PART/refund"))
            .and(body_json(json!({ "amount": 5000 })))
            .respond_with(ResponseTemplate::new(200).set_body_json(partial_response))
            .expect(1)
            .mount(&server)
            .await;

        let partial_url = format!("{}/v1/payments/pay_01J9REF_PART/refund", server.uri());
        let payload = RefundPaymentRequest { amount: 5000 };
        let partial_payment = execute_with_retry(1, || async {
            let resp = http.post(&partial_url).json(&payload).send().await?;
            parse_payment_response(resp).await
        })
        .await
        .expect("partial refund succeeds");

        assert_eq!(partial_payment.refunded, 5000);
        assert!(partial_payment.refunded_at.is_some());
    }
}
