//! Hermetic local HTTP mock responder (`MoyasarMockServer`) for offline CI tests.

use serde_json::{Value, json};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::test_utils::fixtures::FIXTURE_TIMESTAMP;

/// Hermetic local HTTP mock server bound to `127.0.0.1` for offline CI conformance tests.
///
/// # Examples
///
/// ```
/// use some_moyasar::test_utils::MoyasarMockServer;
///
/// let rt = tokio::runtime::Runtime::new().unwrap();
/// rt.block_on(async {
///     let server = MoyasarMockServer::start().await;
///     server.mount_default_routes().await;
///     assert!(server.uri().starts_with("http://127.0.0.1:"));
/// });
/// ```
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
        self.mount_payment_routes(FIXTURE_TIMESTAMP).await;
        self.mount_intent_and_token_routes(FIXTURE_TIMESTAMP).await;
    }

    async fn mount_payment_routes(&self, now: &str) {
        let payment = build_mock_payment_json("pay_mock_01", "paid", 0, now);
        let refunded = build_mock_payment_json("pay_mock_01", "refunded", 15000, now);
        let voided = build_mock_payment_json("pay_mock_01", "voided", 0, now);

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
            .respond_with(ResponseTemplate::new(200).set_body_json(refunded))
            .mount(&self.inner)
            .await;

        Mock::given(method("POST"))
            .and(path_regex(r"^/v1/payments/[^/]+/void$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(voided))
            .mount(&self.inner)
            .await;
    }

    async fn mount_intent_and_token_routes(&self, now: &str) {
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

fn build_mock_payment_json(id: &str, status: &str, refunded: u64, now: &str) -> Value {
    let captured = if status == "voided" { 0 } else { 15000 };
    let captured_fmt = if status == "voided" {
        "0.00 SAR"
    } else {
        "150.00 SAR"
    };
    let refunded_fmt = if refunded > 0 {
        "150.00 SAR"
    } else {
        "0.00 SAR"
    };
    json!({
        "id": id,
        "status": status,
        "amount": 15000,
        "amount_format": "150.00 SAR",
        "fee": 0,
        "fee_format": "0.00 SAR",
        "refunded": refunded,
        "refunded_format": refunded_fmt,
        "captured": captured,
        "captured_format": captured_fmt,
        "currency": "SAR",
        "source": {"type": "creditcard"},
        "refunded_at": (status == "refunded").then_some(now),
        "voided_at": (status == "voided").then_some(now),
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

        #[cfg(all(
            feature = "payments",
            feature = "payment-intents",
            feature = "tokens"
        ))]
        verify_sdk_services_against_mock(&client).await;
    }

    #[cfg(all(
        feature = "payments",
        feature = "payment-intents",
        feature = "tokens"
    ))]
    async fn verify_sdk_services_against_mock(client: &MoyasarClient<SecretKey>) {
        use crate::kernel::types::Currency;
        use crate::resources::payment_intents::CreatePaymentIntentParams;
        use crate::resources::payments::{PaymentId, PaymentStatus};
        use crate::resources::tokens::{TokenId, TokenStatus};

        let pid = PaymentId::new("pay_mock_01").unwrap();
        let fetched = client.payments().retrieve(&pid).await.unwrap();
        assert_eq!(fetched.id, "pay_mock_01");
        assert_eq!(fetched.status, PaymentStatus::Paid);

        let refunded = client.payments().refund("pay_mock_01", None).await.unwrap();
        assert_eq!(refunded.status, PaymentStatus::Refunded);
        assert_eq!(refunded.refunded, 15000);

        let voided = client.payments().void("pay_mock_01").await.unwrap();
        assert_eq!(voided.status, PaymentStatus::Voided);

        let intent = client
            .payment_intents()
            .create(CreatePaymentIntentParams::new(
                15000,
                Currency::SAR,
                "https://merchant.example/cb",
            ))
            .await
            .unwrap();
        assert_eq!(intent.id.as_str(), "pi_mock_01");

        let token = client
            .tokens()
            .retrieve(&TokenId::new("token_mock_01").unwrap())
            .await
            .unwrap();
        assert_eq!(token.status, TokenStatus::Active);
    }
}
