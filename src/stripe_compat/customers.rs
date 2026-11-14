//! Pluggable `CustomerVault` trait and `CustomersService` metadata injector.

use std::future::Future;
use std::pin::Pin;

use crate::kernel::auth::{AuthKey, SecretAuthKey};
use crate::kernel::client::MoyasarClient;
use crate::kernel::error::MoyasarError;
use crate::kernel::metadata::Metadata;
use crate::kernel::types::Currency;
use crate::resources::payments::{CreatePaymentParams, Payment, PaymentSourceRequest, TokenSource};
use crate::resources::tokens::TokenId;

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
    /// Optional merchant metadata (will have `customer_id` injected automatically).
    pub metadata: Option<Metadata>,
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
        let source = TokenSource::new(token_id.as_str())?;
        let req = CreatePaymentParams::builder()
            .amount(params.amount)
            .currency(params.currency)
            .payment_source(PaymentSourceRequest::Token(source))
            .metadata(enriched)
            .build()?;
        self.client.payments().create(&req).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_8_1_0_04_injects_customer_id_metadata() {
        let meta = inject_customer_metadata(None, "cus_101").expect("metadata injected");
        assert_eq!(meta.get("customer_id"), Some("cus_101"));
    }
}
