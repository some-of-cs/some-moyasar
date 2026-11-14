//! Deterministic UUIDv5 subscription cycle billing (`SubscriptionsService`).

use std::borrow::Borrow;

use crate::kernel::auth::{AuthKey, SecretAuthKey};
use crate::kernel::client::MoyasarClient;
use crate::kernel::error::MoyasarError;
use crate::kernel::idempotency_v5::derive_subscription_given_id;
use crate::kernel::types::Currency;
use crate::resources::payments::{CreatePaymentParams, Payment, PaymentSourceRequest, TokenSource};
use crate::resources::tokens::{TokenId, require_active_token};
use crate::stripe_compat::subscriptions_model::{Subscription, SubscriptionCycle};

/// Parameters for charging a single subscription billing cycle off-session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubscriptionCycleChargeParams<'a> {
    /// Merchant subscription identifier.
    pub subscription_id: &'a str,
    /// Monotonic billing cycle index.
    pub cycle_index: u64,
    /// Minor-unit charge amount.
    pub amount: u64,
    /// ISO 4217 currency (defaults to [`Currency::SAR`] in [`Self::new`]).
    pub currency: Currency,
    /// Saved card token identifier (`token_...`).
    pub token_id: &'a TokenId,
}

impl<'a> SubscriptionCycleChargeParams<'a> {
    /// Creates new cycle charge parameters (defaulting to [`Currency::SAR`]).
    #[must_use]
    pub const fn new(
        subscription_id: &'a str,
        cycle_index: u64,
        amount: u64,
        token_id: &'a TokenId,
    ) -> Self {
        Self {
            subscription_id,
            cycle_index,
            amount,
            currency: Currency::SAR,
            token_id,
        }
    }

    /// Creates cycle charge parameters from a [`Subscription`] and [`SubscriptionCycle`].
    #[must_use]
    pub fn from_cycle(subscription: &'a Subscription, cycle: &'a SubscriptionCycle) -> Self {
        Self {
            subscription_id: &subscription.id,
            cycle_index: cycle.cycle_index,
            amount: cycle.amount,
            currency: cycle.currency,
            token_id: &subscription.token_id,
        }
    }

    /// Overrides the billing currency (defaults to [`Currency::SAR`]).
    #[must_use]
    pub const fn currency(mut self, currency: Currency) -> Self {
        self.currency = currency;
        self
    }
}

/// Virtual Stripe-compatible subscriptions service (`client.subscriptions()`).
///
/// # Security
///
/// Off-session recurring charges require a [`SecretKey`](crate::kernel::auth::SecretKey)
/// and fail to compile on `MoyasarClient<PublishableKey>`:
///
/// ```compile_fail
/// use some_moyasar::prelude::{MoyasarClient, PublishableKey, TokenId};
/// use some_moyasar::stripe_compat::SubscriptionCycleChargeParams;
///
/// # async fn demo() {
/// let key = PublishableKey::new("pk_test_123").unwrap();
/// let client = MoyasarClient::new(key).unwrap();
/// let token = TokenId::new("token_abc").unwrap();
/// let params = SubscriptionCycleChargeParams::new("sub_1", 1, 29900, &token);
/// let _ = client.subscriptions().charge_cycle(params).await;
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
    /// Verifies that `params.token_id` is [`TokenStatus::Active`](crate::resources::tokens::TokenStatus::Active)
    /// before submitting `POST /v1/payments` with `three_ds: Some(false)`.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::Api`] with
    /// [`ApiErrorType::InvalidRequestError`](crate::kernel::error::ApiErrorType::InvalidRequestError)
    /// when the token is not `Active`, or propagates HTTP/API errors.
    pub async fn charge_cycle<'a>(
        &self,
        params: impl Borrow<SubscriptionCycleChargeParams<'a>>,
    ) -> Result<Payment, MoyasarError> {
        let params = params.borrow();
        let token = self.client.tokens().retrieve(params.token_id).await?;
        require_active_token(token.status)?;

        let given_id = derive_subscription_given_id(params.subscription_id, params.cycle_index);
        let mut source = TokenSource::new(params.token_id.as_str())?;
        source.three_ds = Some(false);

        let req = CreatePaymentParams::builder()
            .amount(params.amount)
            .currency(params.currency)
            .given_id(given_id)
            .payment_source(PaymentSourceRequest::Token(source))
            .build()?;

        self.client.payments().create(&req).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::error::ApiErrorType;

    fn token_response(id: &str, status: &str) -> Value {
        json!({
            "id": id,
            "status": status,
            "number": "411111XXXXXX1111",
            "company": "visa",
            "gateway_id": "gw_1",
            "created_at": "2026-11-14T10:00:00Z"
        })
    }

    fn cycle_payment_response() -> Value {
        json!({
            "id": "pay_sub_4", "status": "paid", "amount": 29900,
            "amount_format": "299.00 SAR", "fee": 0, "fee_format": "0.00 SAR",
            "refunded": 0, "refunded_format": "0.00 SAR", "captured": 29900,
            "captured_format": "299.00 SAR", "currency": "SAR",
            "source": {"type": "token"},
            "created_at": "2026-11-14T10:00:00Z", "updated_at": "2026-11-14T10:00:00Z"
        })
    }

    #[tokio::test]
    async fn tc_8_2_0_01_active_token_uses_deterministic_given_id() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/tokens/token_abc"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(token_response("token_abc", "active")),
            )
            .mount(&server)
            .await;

        let expected_given_id = derive_subscription_given_id("sub_pro_9921", 4);
        Mock::given(method("POST"))
            .and(path("/v1/payments"))
            .and(body_partial_json(json!({
                "given_id": expected_given_id,
                "source": {"type": "token", "token": "token_abc", "3ds": false}
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(cycle_payment_response()))
            .expect(2)
            .mount(&server)
            .await;

        let client = MoyasarClient::builder(SecretKey::new("sk_test_123").unwrap())
            .base_url(server.uri())
            .build()
            .unwrap();
        let good_tok = TokenId::new("token_abc").unwrap();
        let params = SubscriptionCycleChargeParams::new("sub_pro_9921", 4, 29900, &good_tok)
            .currency(Currency::SAR);

        let p1 = client.subscriptions().charge_cycle(params).await.unwrap();
        let p2 = client.subscriptions().charge_cycle(&params).await.unwrap();
        assert_eq!(p1.id, "pay_sub_4");
        assert_eq!(p2.id, "pay_sub_4");
    }

    #[tokio::test]
    async fn tc_8_2_0_02_inactive_token_rejected_pre_charge() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/tokens/token_inactive"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(token_response("token_inactive", "inactive")),
            )
            .mount(&server)
            .await;

        let client = MoyasarClient::builder(SecretKey::new("sk_test_123").unwrap())
            .base_url(server.uri())
            .build()
            .unwrap();
        let bad_tok = TokenId::new("token_inactive").unwrap();
        let params = SubscriptionCycleChargeParams::new("sub_pro_9921", 4, 29900, &bad_tok);
        let err = client
            .subscriptions()
            .charge_cycle(params)
            .await
            .expect_err("inactive token rejected pre-charge");
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            }
        ));
    }
}
