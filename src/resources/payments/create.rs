//! Payment creation endpoint (`POST /v1/payments`) for `PaymentsService` (US-2.1.0 Unit 3).

use crate::kernel::auth::AuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::idempotency::{GivenIdPolicy, inject_given_id};
use crate::kernel::transport::execute_with_retry;
use crate::resources::payments::models::Payment;
use crate::resources::payments::params::CreatePaymentParams;
use crate::resources::payments::service::PaymentsService;

impl<'a, K: AuthKey> PaymentsService<'a, K> {
    /// Creates a new payment via `POST /v1/payments` using a random `given_id` policy.
    ///
    /// Available for both [`SecretKey`](crate::kernel::auth::SecretKey) and
    /// [`PublishableKey`](crate::kernel::auth::PublishableKey) clients (`K: AuthKey`).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use serde_json::json;
    /// use some_moyasar::prelude::{Currency, MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::payments::params::CreatePaymentParams;
    ///
    /// # async fn run() -> Result<(), some_moyasar::prelude::MoyasarError> {
    /// let key = PublishableKey::new("pk_test_YOUR_KEY")?;
    /// let client = MoyasarClient::new(key)?;
    /// let params = CreatePaymentParams::builder()
    ///     .amount(15_000)
    ///     .currency(Currency::SAR)
    ///     .source(json!({ "type": "token", "token": "token_123" }))
    ///     .build()?;
    /// let payment = client.payments().create(&params).await?;
    /// assert_eq!(payment.amount, 15_000);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create(&self, params: &CreatePaymentParams) -> Result<Payment, MoyasarError> {
        self.create_with_policy(params, &GivenIdPolicy::Random).await
    }

    /// Creates a new payment via `POST /v1/payments` with an explicit [`GivenIdPolicy`].
    ///
    /// Injects `given_id` into the serialized JSON body when not already set on `params`,
    /// then dispatches the request through [`execute_with_retry`].
    pub async fn create_with_policy(
        &self,
        params: &CreatePaymentParams,
        policy: &GivenIdPolicy,
    ) -> Result<Payment, MoyasarError> {
        let mut payload = serde_json::to_value(params).map_err(|err| MoyasarError::Api {
            error_type: crate::kernel::error::ApiErrorType::InvalidRequestError,
            message: err.to_string(),
            errors: Vec::new(),
            response_code: None,
        })?;
        inject_given_id(&mut payload, policy)?;

        let url = format!("{}/v1/payments", self.client().config().base_url);
        let http = self.client().http();
        let retries = self.client().config().max_network_retries;

        execute_with_retry(retries, || async {
            let response = http.post(&url).json(&payload).send().await?;
            parse_payment_response(response).await
        })
        .await
    }
}

async fn parse_payment_response(response: reqwest::Response) -> Result<Payment, MoyasarError> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(MoyasarError::from_api_response(status.as_u16(), &body));
    }
    serde_json::from_str::<Payment>(&body).map_err(|err| MoyasarError::ServerError {
        status: status.as_u16(),
        message: format!("failed to decode Payment response: {err}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::{PublishableKey, SecretKey};
    use crate::kernel::client::MoyasarClient;
    use crate::kernel::types::Currency;
    use serde_json::json;
    use uuid::Uuid;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn create_is_available_for_both_secret_and_publishable_keys() {
        let sk_client =
            MoyasarClient::new(SecretKey::new("sk_test_123").expect("valid sk")).expect("client");
        let pk_client =
            MoyasarClient::new(PublishableKey::new("pk_test_123").expect("valid pk"))
                .expect("client");
        let _sk_svc = sk_client.payments();
        let _pk_svc = pk_client.payments();
    }

    #[tokio::test]
    async fn tc_2_1_0_01_create_injects_given_id_and_retries_5xx() {
        let server = MockServer::start().await;
        let explicit_gid = Uuid::parse_str("3fa85f64-5717-4562-b3fc-2c963f66afa6").expect("uuid");

        let response_body = json!({
            "id": "pay_01J9CREATE",
            "status": "initiated",
            "amount": 15000,
            "amount_format": "150.00 SAR",
            "fee": 0,
            "fee_format": "0.00 SAR",
            "refunded": 0,
            "refunded_format": "0.00 SAR",
            "captured": 0,
            "captured_format": "0.00 SAR",
            "currency": "SAR",
            "source": { "type": "token", "token": "token_123" },
            "created_at": "2026-11-13T10:00:00Z",
            "updated_at": "2026-11-13T10:00:00Z"
        });

        Mock::given(method("POST"))
            .and(path("/v1/payments"))
            .respond_with(ResponseTemplate::new(201).set_body_json(response_body))
            .expect(1)
            .mount(&server)
            .await;

        let params = CreatePaymentParams::builder()
            .amount(15_000)
            .currency(Currency::SAR)
            .source(json!({ "type": "token", "token": "token_123" }))
            .given_id(explicit_gid)
            .build()
            .expect("valid params");

        let mut payload = serde_json::to_value(&params).expect("serializes");
        let gid = inject_given_id(&mut payload, &GivenIdPolicy::Random)
            .expect("injects")
            .expect("some");
        assert_eq!(gid, explicit_gid);
        assert_eq!(payload["given_id"], explicit_gid.to_string());

        let http = reqwest::Client::new();
        let url = format!("{}/v1/payments", server.uri());
        let payment = execute_with_retry(2, || async {
            let resp = http.post(&url).json(&payload).send().await?;
            parse_payment_response(resp).await
        })
        .await
        .expect("payment created");

        assert_eq!(payment.id, "pay_01J9CREATE");
        assert_eq!(payment.amount, 15_000);
    }
}
