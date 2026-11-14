//! Hermetic local HTTP mock responder (`MoyasarMockServer`) for offline CI tests.

use chrono::Utc;
use serde_json::{Value, json};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Hermetic local HTTP mock server bound to `127.0.0.1` for offline CI conformance tests.
pub struct MoyasarMockServer {
    inner: MockServer,
}

impl MoyasarMockServer {
    /// Starts a new [`MoyasarMockServer`] on `127.0.0.1:0` with zero outbound network access.
    pub async fn start() -> Self {
        Self {
            inner: MockServer::start().await,
        }
    }

    /// Returns the local HTTP base URI (e.g. `http://127.0.0.1:12345`).
    #[must_use]
    pub fn uri(&self) -> String {
        self.inner.uri()
    }

    /// Returns a reference to the underlying [`wiremock::MockServer`].
    #[must_use]
    pub const fn server(&self) -> &MockServer {
        &self.inner
    }

    /// Mounts default deterministic responders for payments, intents, and tokens.
    pub async fn mount_default_routes(&self) {
        let now = Utc::now().to_rfc3339();
        let payment = sample_payment_json("pay_mock_01", "paid", 15000, &now);

        Mock::given(method("POST"))
            .and(path("/v1/payments"))
            .respond_with(ResponseTemplate::new(201).set_body_json(payment.clone()))
            .mount(&self.inner)
            .await;

        Mock::given(method("GET"))
            .and(path_regex(r"^/v1/payments/[^/]+$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(payment))
            .mount(&self.inner)
            .await;

        Mock::given(method("POST"))
            .and(path_regex(r"^/v1/payments/[^/]+/refund$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(sample_refunded_json(&now)))
            .mount(&self.inner)
            .await;

        Mock::given(method("POST"))
            .and(path_regex(r"^/v1/payments/[^/]+/void$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(sample_voided_json(&now)))
            .mount(&self.inner)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/payment_intents"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "pi_mock_01",
                "status": "created",
                "amount": 15000,
                "amount_format": "150.00 SAR",
                "currency": "SAR",
                "client_secret": "pi_secret_mock",
                "expires_at": now,
                "created_at": now,
                "updated_at": now
            })))
            .mount(&self.inner)
            .await;

        Mock::given(method("GET"))
            .and(path_regex(r"^/v1/tokens/token_[^/]+$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "token_mock_01",
                "status": "active",
                "number": "411111XXXXXX1111",
                "company": "visa",
                "gateway_id": "gw_mock",
                "created_at": now
            })))
            .mount(&self.inner)
            .await;
    }
}

fn sample_payment_json(id: &str, status: &str, amount: u64, now: &str) -> Value {
    json!({
        "id": id,
        "status": status,
        "amount": amount,
        "amount_format": "150.00 SAR",
        "fee": 0,
        "fee_format": "0.00 SAR",
        "refunded": 0,
        "refunded_format": "0.00 SAR",
        "captured": amount,
        "captured_format": "150.00 SAR",
        "currency": "SAR",
        "source": {"type": "creditcard"},
        "created_at": now,
        "updated_at": now
    })
}

fn sample_refunded_json(now: &str) -> Value {
    json!({
        "id": "pay_mock_01",
        "status": "refunded",
        "amount": 15000,
        "amount_format": "150.00 SAR",
        "fee": 0,
        "fee_format": "0.00 SAR",
        "refunded": 15000,
        "refunded_format": "150.00 SAR",
        "captured": 15000,
        "captured_format": "150.00 SAR",
        "currency": "SAR",
        "source": {"type": "creditcard"},
        "refunded_at": now,
        "created_at": now,
        "updated_at": now
    })
}

fn sample_voided_json(now: &str) -> Value {
    json!({
        "id": "pay_mock_01",
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
        "source": {"type": "creditcard"},
        "voided_at": now,
        "created_at": now,
        "updated_at": now
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::client::MoyasarClient;

    #[tokio::test]
    async fn tc_8_3_0_03_mock_server_starts_hermetically_on_loopback() {
        let server = MoyasarMockServer::start().await;
        server.mount_default_routes().await;
        assert!(server.uri().starts_with("http://127.0.0.1:"));

        let client = MoyasarClient::builder(SecretKey::new("sk_test_mock").unwrap())
            .base_url(server.uri())
            .build()
            .expect("loopback mock server URL allowed in test-utils");
        let resp = client
            .http()
            .get(format!("{}/v1/payments/pay_mock_01", server.uri()))
            .send()
            .await
            .expect("local mock responds");
        assert!(resp.status().is_success());
    }
}
