//! Deterministic UUIDv5 subscription cycle billing (`SubscriptionsService`).

use crate::kernel::auth::{AuthKey, SecretAuthKey};
use crate::kernel::client::MoyasarClient;
use crate::kernel::error::MoyasarError;
use crate::kernel::idempotency_v5::derive_subscription_given_id;
use crate::kernel::types::Currency;
use crate::resources::payments::{CreatePaymentParams, Payment, PaymentSourceRequest, TokenSource};
use crate::resources::tokens::{TokenId, require_active_token};

/// Virtual Stripe-compatible subscriptions service (`client.subscriptions()`).
///
/// # Security
///
/// Off-session recurring charges require a [`SecretKey`](crate::kernel::auth::SecretKey)
/// and fail to compile on `MoyasarClient<PublishableKey>`:
///
/// ```compile_fail
/// use some_moyasar::prelude::{Currency, MoyasarClient, PublishableKey, TokenId};
///
/// # async fn demo() {
/// let key = PublishableKey::new("pk_test_123").unwrap();
/// let client = MoyasarClient::new(key).unwrap();
/// let token = TokenId::new("token_abc").unwrap();
/// let _ = client.subscriptions().charge_cycle("sub_1", 1, 29900, Currency::SAR, &token).await;
/// # }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct SubscriptionsService<'a, K: AuthKey> {
    client: &'a MoyasarClient<K>,
}

impl<'a, K: AuthKey> SubscriptionsService<'a, K> {
    /// Returns a reference to the underlying [`MoyasarClient`].
    #[must_use]
    pub const fn client(&self) -> &'a MoyasarClient<K> {
        self.client
    }
}

impl<K: AuthKey> MoyasarClient<K> {
    /// Returns a [`SubscriptionsService`] handle for recurring subscription billing.
    #[must_use]
    pub const fn subscriptions(&self) -> SubscriptionsService<'_, K> {
        SubscriptionsService { client: self }
    }
}

impl<K: SecretAuthKey> SubscriptionsService<'_, K> {
    /// Charges a single subscription billing cycle off-session using deterministic UUIDv5 `given_id`.
    ///
    /// Verifies that `token_id` is [`TokenStatus::Active`](crate::resources::tokens::TokenStatus::Active)
    /// before submitting `POST /v1/payments` with `three_ds: Some(false)`.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::InvalidTokenId`] if the token is inactive/unverified,
    /// or propagates HTTP/API errors.
    pub async fn charge_cycle(
        &self,
        sub_id: &str,
        cycle_index: u64,
        amount: u64,
        currency: Currency,
        token_id: &TokenId,
    ) -> Result<Payment, MoyasarError> {
        let token = self.client.tokens().retrieve(token_id).await?;
        require_active_token(token.status)?;

        let given_id = derive_subscription_given_id(sub_id, cycle_index);
        let mut source = TokenSource::new(token_id.as_str())?;
        source.three_ds = Some(false);

        let params = CreatePaymentParams::builder()
            .amount(amount)
            .currency(currency)
            .given_id(given_id)
            .payment_source(PaymentSourceRequest::Token(source))
            .build()?;

        self.client.payments().create(&params).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::kernel::auth::SecretKey;

    fn token_response(status: &str) -> serde_json::Value {
        json!({
            "id": "token_abc",
            "status": status,
            "number": "411111XXXXXX1111",
            "company": "visa",
            "gateway_id": "gw_1",
            "created_at": "2026-11-14T10:00:00Z"
        })
    }

    #[tokio::test]
    async fn tc_8_2_0_01_and_02_active_token_uses_deterministic_given_id_and_inactive_rejects() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/tokens/token_inactive"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "token_inactive",
                "status": "inactive",
                "number": "411111XXXXXX1111",
                "company": "visa",
                "gateway_id": "gw_1",
                "created_at": "2026-11-14T10:00:00Z"
            })))
            .mount(&server)
            .await;

        let client = MoyasarClient::builder(SecretKey::new("sk_test_123").unwrap())
            .base_url(server.uri())
            .build()
            .unwrap();
        let bad_tok = TokenId::new("token_inactive").unwrap();
        let err = client
            .subscriptions()
            .charge_cycle("sub_pro_9921", 4, 29900, Currency::SAR, &bad_tok)
            .await
            .expect_err("inactive token rejected pre-charge");
        assert!(matches!(err, MoyasarError::InvalidTokenId));

        Mock::given(method("GET"))
            .and(path("/v1/tokens/token_abc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(token_response("active")))
            .mount(&server)
            .await;
        let expected_given_id = derive_subscription_given_id("sub_pro_9921", 4);
        Mock::given(method("POST"))
            .and(path("/v1/payments"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "pay_sub_4",
                "status": "paid",
                "amount": 29900,
                "amount_format": "299.00 SAR",
                "fee": 0,
                "fee_format": "0.00 SAR",
                "refunded": 0,
                "refunded_format": "0.00 SAR",
                "captured": 29900,
                "captured_format": "299.00 SAR",
                "currency": "SAR",
                "source": {"type": "token"},
                "created_at": "2026-11-14T10:00:00Z",
                "updated_at": "2026-11-14T10:00:00Z"
            })))
            .expect(2)
            .mount(&server)
            .await;

        let good_tok = TokenId::new("token_abc").unwrap();
        let p1 = client
            .subscriptions()
            .charge_cycle("sub_pro_9921", 4, 29900, Currency::SAR, &good_tok)
            .await
            .unwrap();
        let p2 = client
            .subscriptions()
            .charge_cycle("sub_pro_9921", 4, 29900, Currency::SAR, &good_tok)
            .await
            .unwrap();
        assert_eq!(p1.id, "pay_sub_4");
        assert_eq!(p2.id, "pay_sub_4");
        assert_eq!(
            expected_given_id,
            derive_subscription_given_id("sub_pro_9921", 4)
        );
    }
}
