//! Pluggable `CustomerVault` trait and `CustomersService` metadata injector.

use std::future::Future;
use std::pin::Pin;

use uuid::Uuid;

use crate::kernel::auth::{AuthKey, SecretAuthKey};
use crate::kernel::client::MoyasarClient;
use crate::kernel::error::MoyasarError;
use crate::kernel::idempotency_v5::MOYASAR_SUBSCRIPTION_NAMESPACE;
use crate::kernel::metadata::Metadata;
use crate::kernel::types::Currency;
use crate::resources::payments::{CreatePaymentParams, Payment, PaymentSourceRequest, TokenSource};
use crate::resources::tokens::TokenId;
pub use crate::stripe_compat::in_memory_vault::{
    DEFAULT_MAX_CUSTOMERS, DEFAULT_MAX_TOKENS_PER_CUSTOMER, InMemoryCustomerVault,
};

/// Pluggable persistence trait mapping merchant customer IDs to Moyasar [`TokenId`]s.
///
/// # PCI-DSS 4.0 Hygiene
///
/// Implementations store only [`TokenId`] references (`token_...`) and merchant
/// customer identifiers—never raw PANs, CVCs, or expiry dates.
pub trait CustomerVault: Send + Sync {
    /// Associates `token_id` with `customer_id`.
    fn associate<'a>(
        &'a self,
        customer_id: &'a str,
        token_id: TokenId,
    ) -> Pin<Box<dyn Future<Output = Result<(), MoyasarError>> + Send + 'a>>;

    /// Resolves all active [`TokenId`]s associated with `customer_id`.
    fn resolve_tokens<'a>(
        &'a self,
        customer_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<TokenId>, MoyasarError>> + Send + 'a>>;

    /// Deactivates/removes `token_id` from active resolution.
    fn deactivate<'a>(
        &'a self,
        token_id: &'a TokenId,
    ) -> Pin<Box<dyn Future<Output = Result<(), MoyasarError>> + Send + 'a>>;
}

/// Parameters for charging a customer's saved token via [`CustomersService::charge`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerChargeParams<'a> {
    /// Merchant customer identifier.
    pub customer_id: &'a str,
    /// Charge amount in minor units.
    pub amount: u64,
    /// ISO 4217 currency.
    pub currency: Currency,
    /// Optional caller-supplied `given_id` idempotency UUID.
    pub given_id: Option<Uuid>,
    /// Optional merchant metadata (will have `customer_id` injected automatically).
    pub metadata: Option<Metadata>,
}

impl<'a> CustomerChargeParams<'a> {
    /// Creates new customer charge parameters for `customer_id`, `amount`, and `currency`.
    #[must_use]
    pub const fn new(customer_id: &'a str, amount: u64, currency: Currency) -> Self {
        Self {
            customer_id,
            amount,
            currency,
            given_id: None,
            metadata: None,
        }
    }

    /// Sets an explicit `given_id` idempotency key for the payment request.
    #[must_use]
    pub const fn given_id(mut self, given_id: Uuid) -> Self {
        self.given_id = Some(given_id);
        self
    }

    /// Sets initial merchant [`Metadata`] (will have `customer_id` injected automatically).
    #[must_use]
    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Injects `metadata["customer_id"] = customer_id` into [`Metadata`].
///
/// # Errors
///
/// Returns [`MoyasarError`] if metadata key/value limits (30 keys, 40/500 chars) are exceeded.
pub fn inject_customer_metadata(
    metadata: Option<Metadata>,
    customer_id: &str,
) -> Result<Metadata, MoyasarError> {
    let mut map = metadata.unwrap_or_default();
    map.insert("customer_id", customer_id)?;
    Ok(map)
}

/// Customer-scoped saved-token charging facade backed by a [`CustomerVault`].
///
/// # Security
///
/// Charging a customer's saved token requires a [`SecretKey`](crate::kernel::auth::SecretKey)
/// and fails to compile on `MoyasarClient<PublishableKey>`:
///
/// ```compile_fail
/// use some_moyasar::prelude::{Currency, MoyasarClient, PublishableKey};
/// use some_moyasar::stripe_compat::customers::CustomerChargeParams;
/// use some_moyasar::stripe_compat::in_memory_vault::InMemoryCustomerVault;
///
/// # async fn demo() {
/// let key = PublishableKey::new("pk_test_123").unwrap();
/// let client = MoyasarClient::new(key).unwrap();
/// let vault = InMemoryCustomerVault::new();
/// let params = CustomerChargeParams::new("cus_101", 15000, Currency::SAR);
/// let _ = client.customers(&vault).charge(&params).await;
/// # }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct CustomersService<'a, K: AuthKey, V: CustomerVault> {
    client: &'a MoyasarClient<K>,
    vault: &'a V,
}

impl<'a, K: AuthKey, V: CustomerVault> CustomersService<'a, K, V> {
    /// Returns a reference to the underlying [`MoyasarClient`].
    #[must_use]
    pub const fn client(&self) -> &'a MoyasarClient<K> {
        self.client
    }

    /// Returns a reference to the underlying [`CustomerVault`].
    #[must_use]
    pub const fn vault(&self) -> &'a V {
        self.vault
    }
}

impl<K: AuthKey> MoyasarClient<K> {
    /// Returns a [`CustomersService`] handle bound to `vault`.
    #[must_use]
    pub const fn customers<'a, V: CustomerVault>(
        &'a self,
        vault: &'a V,
    ) -> CustomersService<'a, K, V> {
        CustomersService {
            client: self,
            vault,
        }
    }
}

impl<K: SecretAuthKey, V: CustomerVault> CustomersService<'_, K, V> {
    /// Associates `token_id` with `customer_id` in the configured [`CustomerVault`].
    ///
    /// # Errors
    ///
    /// Propagates any [`MoyasarError`] from the underlying vault.
    pub async fn attach_token(
        &self,
        customer_id: &str,
        token_id: TokenId,
    ) -> Result<(), MoyasarError> {
        self.vault.associate(customer_id, token_id).await
    }

    /// Charges the first active saved token for `customer_id`, injecting `metadata["customer_id"]`.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::InvalidTokenId`] if no active token is bound to `customer_id`,
    /// or propagates HTTP/validation errors.
    pub async fn charge(&self, params: &CustomerChargeParams<'_>) -> Result<Payment, MoyasarError> {
        let tokens = self.vault.resolve_tokens(params.customer_id).await?;
        let token_id = tokens.first().ok_or(MoyasarError::InvalidTokenId)?;
        let enriched = inject_customer_metadata(params.metadata.clone(), params.customer_id)?;
        let mut source = TokenSource::new(token_id.as_str())?;
        source.three_ds = Some(false);
        let given_id = params
            .given_id
            .unwrap_or_else(|| derive_customer_charge_given_id(params, token_id, &enriched));
        let req = CreatePaymentParams::builder()
            .amount(params.amount)
            .currency(params.currency)
            .given_id(given_id)
            .payment_source(PaymentSourceRequest::Token(source))
            .metadata(enriched)
            .build()?;
        self.client.payments().create(&req).await
    }
}

fn derive_customer_charge_given_id(
    params: &CustomerChargeParams<'_>,
    token_id: &TokenId,
    metadata: &Metadata,
) -> Uuid {
    let meta_json = serde_json::to_string(metadata).unwrap_or_default();
    let scope = format!(
        "customer_charge:{}:{}:{}:{}:{meta_json}",
        params.customer_id,
        token_id.as_str(),
        params.amount,
        params.currency.as_str()
    );
    Uuid::new_v5(&MOYASAR_SUBSCRIPTION_NAMESPACE, scope.as_bytes())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::kernel::auth::SecretKey;

    async fn mount_customer_charge_mock(server: &MockServer, given_id: Uuid) {
        Mock::given(method("POST"))
            .and(path("/v1/payments"))
            .and(body_partial_json(json!({
                "amount": 15000,
                "currency": "SAR",
                "given_id": given_id,
                "metadata": {"customer_id": "cus_101", "order": "ord_55"},
                "source": {"type": "token", "token": "token_cus101", "3ds": false}
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "pay_cus_101", "status": "paid", "amount": 15000,
                "amount_format": "150.00 SAR", "fee": 0, "fee_format": "0.00 SAR",
                "refunded": 0, "refunded_format": "0.00 SAR", "captured": 15000,
                "captured_format": "150.00 SAR", "currency": "SAR",
                "source": {"type": "token"},
                "metadata": {"customer_id": "cus_101", "order": "ord_55"},
                "created_at": "2026-11-14T12:00:00Z", "updated_at": "2026-11-14T12:00:00Z"
            })))
            .expect(1)
            .mount(server)
            .await;
    }

    #[test]
    fn derives_stable_given_id_when_omitted() {
        let tok = TokenId::new("token_cus101").unwrap();
        let meta = inject_customer_metadata(None, "cus_101").unwrap();
        let params = CustomerChargeParams::new("cus_101", 15000, Currency::SAR);
        let id1 = derive_customer_charge_given_id(&params, &tok, &meta);
        let id2 = derive_customer_charge_given_id(&params, &tok, &meta);
        assert_eq!(id1, id2);
    }

    #[test]
    fn tc_8_1_0_04_injects_customer_id_metadata() {
        let meta = inject_customer_metadata(None, "cus_101").expect("metadata injected");
        assert_eq!(meta.get("customer_id"), Some("cus_101"));
    }

    #[tokio::test]
    async fn wiremock_customers_service_attaches_and_charges_with_metadata_given_id_and_no_3ds() {
        let server = MockServer::start().await;
        let given_id = Uuid::new_v4();
        mount_customer_charge_mock(&server, given_id).await;

        let client = MoyasarClient::builder(SecretKey::new("sk_test_123").unwrap())
            .base_url(server.uri())
            .build()
            .unwrap();
        let vault = InMemoryCustomerVault::new();
        let customers = client.customers(&vault);

        let missing = customers
            .charge(&CustomerChargeParams::new("cus_101", 15000, Currency::SAR))
            .await
            .expect_err("missing token must fail");
        assert!(matches!(missing, MoyasarError::InvalidTokenId));

        customers
            .attach_token("cus_101", TokenId::new("token_cus101").unwrap())
            .await
            .unwrap();
        let mut initial_meta = Metadata::new();
        initial_meta.insert("order", "ord_55").unwrap();
        let params = CustomerChargeParams::new("cus_101", 15000, Currency::SAR)
            .given_id(given_id)
            .metadata(initial_meta);

        let payment = customers.charge(&params).await.unwrap();
        assert_eq!(payment.id, "pay_cus_101");
        assert_eq!(payment.metadata.get("customer_id"), Some("cus_101"));
    }

    #[tokio::test]
    async fn enforces_vault_capacity_limits() {
        let vault = InMemoryCustomerVault::with_capacity_limits(1, 2);
        let t1 = TokenId::new("token_cap1").unwrap();
        let t2 = TokenId::new("token_cap2").unwrap();
        let t3 = TokenId::new("token_cap3").unwrap();

        vault.associate("cus_1", t1.clone()).await.unwrap();
        vault.associate("cus_1", t1).await.unwrap();
        vault.associate("cus_1", t2).await.unwrap();

        let tok_err = vault
            .associate("cus_1", t3.clone())
            .await
            .expect_err("exceeding max_tokens_per_customer must fail");
        assert!(!tok_err.is_retryable());

        let cus_err = vault
            .associate("cus_2", t3)
            .await
            .expect_err("exceeding max_customers must fail");
        assert!(!cus_err.is_retryable());
    }
}
