//! STC Pay OTP verification endpoint (US-2.4.0 Unit 2).

use core::fmt;

use serde::{Deserialize, Serialize};

use crate::kernel::auth::{AuthKey, SecretAuthKey};
use crate::kernel::client::MoyasarClient;
use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::transport::{decode_json_response, execute_with_retry};
use crate::resources::payments::service::PaymentsService;

/// Request payload posted to the server-provided STC Pay `transaction_url`.
///
/// The `otp_value` field is redacted in [`fmt::Debug`] output to prevent
/// accidental credential leakage in logs or telemetry.
#[derive(Clone, Serialize)]
pub struct StcPayOtpRequest<'a> {
    /// One-time password entered by the customer.
    pub otp_value: &'a str,
}

impl fmt::Debug for StcPayOtpRequest<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StcPayOtpRequest")
            .field("otp_value", &"[REDACTED]")
            .finish()
    }
}

/// Payment response returned after submitting an STC Pay OTP.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StcPayOtpPayment {
    /// Unique Moyasar payment identifier.
    pub id: String,
    /// Payment lifecycle status (`"paid"`, `"failed"`, `"initiated"`, etc.).
    pub status: String,
    /// Payment amount in the smallest currency unit (e.g., Halalas).
    pub amount: i64,
    /// Pre-formatted human-readable payment amount string (e.g., `"100.00 SAR"`).
    #[serde(default)]
    pub amount_format: Option<String>,
    /// Processing fee in minor units.
    #[serde(default)]
    pub fee: Option<i64>,
    /// Pre-formatted processing fee string.
    #[serde(default)]
    pub fee_format: Option<String>,
    /// Total refunded amount in minor units.
    #[serde(default)]
    pub refunded: Option<i64>,
    /// Pre-formatted refunded amount string.
    #[serde(default)]
    pub refunded_format: Option<String>,
    /// Total captured amount in minor units.
    #[serde(default)]
    pub captured: Option<i64>,
    /// Pre-formatted captured amount string.
    #[serde(default)]
    pub captured_format: Option<String>,
    /// ISO 4217 currency code (e.g., `"SAR"`).
    pub currency: String,
    /// Optional payment description.
    #[serde(default)]
    pub description: Option<String>,
    /// Raw source payload returned by the gateway (redacted in `Debug` output).
    #[serde(default)]
    pub source: Option<serde_json::Value>,
    /// Optional merchant metadata map.
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    /// Optional callback URL.
    #[serde(default)]
    pub callback_url: Option<String>,
    /// Optional associated invoice ID.
    #[serde(default)]
    pub invoice_id: Option<String>,
    /// Optional creation timestamp string.
    #[serde(default)]
    pub created_at: Option<String>,
    /// Optional last-update timestamp string.
    #[serde(default)]
    pub updated_at: Option<String>,
}

impl fmt::Debug for StcPayOtpPayment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StcPayOtpPayment")
            .field("id", &self.id)
            .field("status", &self.status)
            .field("amount", &self.amount)
            .field("amount_format", &self.amount_format)
            .field("fee", &self.fee)
            .field("fee_format", &self.fee_format)
            .field("refunded", &self.refunded)
            .field("refunded_format", &self.refunded_format)
            .field("captured", &self.captured)
            .field("captured_format", &self.captured_format)
            .field("currency", &self.currency)
            .field("description", &self.description)
            .field("source", &"[REDACTED]")
            .field("metadata", &self.metadata)
            .field("callback_url", &self.callback_url)
            .field("invoice_id", &self.invoice_id)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

/// Returns `true` when an error reflects STC Pay's 60-second OTP resend/retry cooldown.
#[must_use]
pub fn is_otp_cooldown_error(err: &MoyasarError) -> bool {
    match err {
        MoyasarError::Api { message, .. } => {
            let lower = message.to_ascii_lowercase();
            lower.contains("cooldown") || lower.contains("60 second") || lower.contains("wait")
        }
        _ => false,
    }
}

impl<'a, K: SecretAuthKey> PaymentsService<'a, K> {
    /// Completes an initiated STC Pay charge by posting the customer's OTP
    /// to the server-provided `transaction_url`.
    ///
    /// Restricted at compile time to [`SecretKey`](crate::kernel::auth::SecretKey) (`K: SecretAuthKey`).
    ///
    /// # Security
    ///
    /// - Enforces same-origin (`scheme`, `host`, and `port`) against `client.base_url()`
    ///   without userinfo or fragments to prevent SSRF and credential exfiltration.
    /// - Uses `transaction_url` verbatim without client-side URL reconstruction.
    /// - Redacts `otp` and `source` in all `Debug` formatting.
    ///
    /// # Errors
    ///
    /// - [`MoyasarError::InvalidBaseUrl`] if `transaction_url` does not match `client.base_url()`'s origin.
    /// - [`MoyasarError::Api`] if `otp` is empty or the gateway rejects the OTP.
    /// - [`MoyasarError::Network`] / [`MoyasarError::ServerError`] on transport failures.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, SecretKey};
    ///
    /// # async fn run() -> Result<(), some_moyasar::prelude::MoyasarError> {
    /// let sk = SecretKey::new("sk_test_your_secret_key")?;
    /// let client = MoyasarClient::new(sk)?;
    /// let verified = client
    ///     .payments()
    ///     .verify_stc_otp(
    ///         "https://api.moyasar.com/v1/stc_pays/pay_123/proceed?otp_token=tok_xyz",
    ///         "123456",
    ///     )
    ///     .await?;
    /// assert_eq!(verified.status, "paid");
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Calling `verify_stc_otp` with a [`PublishableKey`](crate::kernel::auth::PublishableKey)
    /// is rejected at compile time:
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    ///
    /// # async fn example() {
    /// let pk = PublishableKey::new("pk_test_abc123").unwrap();
    /// let client = MoyasarClient::new(pk).unwrap();
    /// let _ = client
    ///     .payments()
    ///     .verify_stc_otp(
    ///         "https://api.moyasar.com/v1/stc_pays/pay_123/proceed?otp_token=tok_xyz",
    ///         "123456",
    ///     )
    ///     .await;
    /// # }
    /// ```
    pub async fn verify_stc_otp(
        &self,
        transaction_url: &str,
        otp: &str,
    ) -> Result<StcPayOtpPayment, MoyasarError> {
        if !is_allowed_stc_otp_url(transaction_url, self.client()) {
            return Err(MoyasarError::InvalidBaseUrl);
        }
        self.verify_stc_otp_inner(transaction_url, otp).await
    }

    async fn verify_stc_otp_inner(
        &self,
        transaction_url: &str,
        otp: &str,
    ) -> Result<StcPayOtpPayment, MoyasarError> {
        let trimmed_otp = otp.trim();
        if trimmed_otp.is_empty() {
            return Err(MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                message: "OTP value must not be empty".to_owned(),
                errors: Vec::new(),
                response_code: None,
            });
        }

        let payload = StcPayOtpRequest {
            otp_value: trimmed_otp,
        };
        let http = self.client().http();
        let max_retries = self.client().config().max_network_retries;

        execute_with_retry(max_retries, || async {
            let response = http.post(transaction_url).json(&payload).send().await?;
            decode_json_response(response).await
        })
        .await
    }
}

fn has_no_userinfo_or_fragment(url: &reqwest::Url) -> bool {
    url.username().is_empty() && url.password().is_none() && url.fragment().is_none()
}

fn has_same_origin(a: &reqwest::Url, b: &reqwest::Url) -> bool {
    (a.scheme(), a.host_str(), a.port_or_known_default())
        == (b.scheme(), b.host_str(), b.port_or_known_default())
}

fn is_allowed_stc_otp_url<K: AuthKey>(transaction_url: &str, client: &MoyasarClient<K>) -> bool {
    let (Ok(parsed), Ok(base)) = (
        reqwest::Url::parse(transaction_url),
        reqwest::Url::parse(client.base_url()),
    ) else {
        return false;
    };
    has_no_userinfo_or_fragment(&parsed) && has_same_origin(&parsed, &base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::client::MoyasarClient;
    use wiremock::matchers::{body_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn redacts_otp_and_payment_source_in_debug_output() {
        let req = StcPayOtpRequest {
            otp_value: "987654",
        };
        let formatted = format!("{req:?}");
        assert!(formatted.contains("[REDACTED]"));
        assert!(!formatted.contains("987654"));

        let payment = StcPayOtpPayment {
            id: "pay_1".to_owned(),
            status: "paid".to_owned(),
            amount: 1000,
            amount_format: None,
            fee: None,
            fee_format: None,
            refunded: None,
            refunded_format: None,
            captured: None,
            captured_format: None,
            currency: "SAR".to_owned(),
            description: None,
            source: Some(serde_json::json!({ "mobile": "0512345678" })),
            metadata: None,
            callback_url: None,
            invoice_id: None,
            created_at: None,
            updated_at: None,
        };
        let pay_dbg = format!("{payment:?}");
        assert!(pay_dbg.contains("[REDACTED]"));
        assert!(!pay_dbg.contains("0512345678"));
    }

    #[tokio::test]
    async fn rejects_plaintext_http_and_cross_origin_transaction_urls() {
        let key = SecretKey::new("sk_test_stcpay_otp").expect("valid key");
        let client = MoyasarClient::new(key).expect("client builds");

        for bad_url in [
            "http://api.moyasar.com/v1/stc_pays/pay_1/proceed?otp_token=abc",
            "https://evil.example.com/v1/stc_pays/pay_1/proceed?otp_token=abc",
            "http://localhost.evil.com/v1/stc_pays/pay_1/proceed",
            "https://user:pass@api.moyasar.com/v1/stc_pays/pay_1/proceed",
            "https://api.moyasar.com/v1/stc_pays/pay_1/proceed#frag",
        ] {
            let err = client
                .payments()
                .verify_stc_otp(bad_url, "123456")
                .await
                .expect_err("must reject invalid or cross-origin transaction_url");

            assert!(matches!(err, MoyasarError::InvalidBaseUrl));
        }
    }

    #[tokio::test]
    async fn tc_2_4_0_03_posts_otp_value_verbatim_and_returns_paid_status() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/stc_pays/pay_123/proceed"))
            .and(query_param("otp_token", "tok_xyz"))
            .and(body_json(serde_json::json!({ "otp_value": "123456" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "pay_123",
                "status": "paid",
                "amount": 10000,
                "amount_format": "100.00 SAR",
                "fee": 150,
                "fee_format": "1.50 SAR",
                "currency": "SAR",
                "description": "STC Pay order"
            })))
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_stcpay_otp").expect("valid key");
        let client = MoyasarClient::builder(key)
            .base_url(server.uri())
            .build()
            .expect("client builds");
        let url = format!(
            "{}/v1/stc_pays/pay_123/proceed?otp_token=tok_xyz",
            server.uri()
        );

        let payment = client
            .payments()
            .verify_stc_otp(&url, "123456")
            .await
            .expect("OTP verification succeeds");

        assert_eq!(payment.id, "pay_123");
        assert_eq!(payment.status, "paid");
        assert_eq!(payment.amount, 10000);
        assert_eq!(payment.amount_format.as_deref(), Some("100.00 SAR"));
        assert_eq!(payment.currency, "SAR");
    }

    #[test]
    fn detects_otp_cooldown_errors() {
        let err = MoyasarError::from_api_response(
            429,
            r#"{"type":"invalid_request_error","message":"Please wait for 60 second cooldown"}"#,
        );
        assert!(is_otp_cooldown_error(&err));
        assert!(!is_otp_cooldown_error(&MoyasarError::InvalidBaseUrl));
    }
}
