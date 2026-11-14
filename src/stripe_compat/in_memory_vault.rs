//! Thread-safe `InMemoryCustomerVault` reference implementation.

use std::collections::BTreeMap;
use std::future::{Future, ready};
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use crate::kernel::error::MoyasarError;
use crate::resources::tokens::TokenId;
use crate::stripe_compat::customers::CustomerVault;

/// Default maximum number of distinct customers stored in [`InMemoryCustomerVault`].
pub const DEFAULT_MAX_CUSTOMERS: usize = 10_000;

/// Default maximum number of saved tokens per customer in [`InMemoryCustomerVault`].
pub const DEFAULT_MAX_TOKENS_PER_CUSTOMER: usize = 32;

/// Thread-safe, in-memory [`CustomerVault`] backed by `Arc<RwLock<...>>` with capacity bounds.
///
/// Suitable for single-instance services and hermetic unit tests. Production
/// distributed deployments can implement [`CustomerVault`] over PostgreSQL or Redis.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::tokens::TokenId;
/// use some_moyasar::stripe_compat::customers::CustomerVault;
/// use some_moyasar::stripe_compat::in_memory_vault::InMemoryCustomerVault;
///
/// let rt = tokio::runtime::Runtime::new().unwrap();
/// rt.block_on(async {
///     let vault = InMemoryCustomerVault::new();
///     let token = TokenId::new("token_abc123").unwrap();
///     vault.associate("cus_101", token.clone()).await.unwrap();
///     assert_eq!(vault.resolve_tokens("cus_101").await.unwrap(), vec![token.clone()]);
///     vault.deactivate(&token).await.unwrap();
///     assert!(vault.resolve_tokens("cus_101").await.unwrap().is_empty());
/// });
/// ```
#[derive(Debug, Clone)]
pub struct InMemoryCustomerVault {
    inner: Arc<RwLock<BTreeMap<String, Vec<TokenId>>>>,
    max_customers: usize,
    max_tokens_per_customer: usize,
}

impl Default for InMemoryCustomerVault {
    fn default() -> Self {
        Self::with_capacity_limits(DEFAULT_MAX_CUSTOMERS, DEFAULT_MAX_TOKENS_PER_CUSTOMER)
    }
}

impl InMemoryCustomerVault {
    /// Default maximum number of distinct customers stored in [`InMemoryCustomerVault`].
    pub const DEFAULT_MAX_CUSTOMERS: usize = DEFAULT_MAX_CUSTOMERS;

    /// Default maximum number of saved tokens per customer in [`InMemoryCustomerVault`].
    pub const DEFAULT_MAX_TOKENS_PER_CUSTOMER: usize = DEFAULT_MAX_TOKENS_PER_CUSTOMER;

    /// Creates an empty [`InMemoryCustomerVault`] with default capacity limits.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an empty [`InMemoryCustomerVault`] with explicit customer and token limits.
    #[must_use]
    pub fn with_capacity_limits(max_customers: usize, max_tokens_per_customer: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(BTreeMap::new())),
            max_customers,
            max_tokens_per_customer,
        }
    }

    /// Returns the configured maximum number of distinct customers.
    #[must_use]
    pub const fn max_customers(&self) -> usize {
        self.max_customers
    }

    /// Returns the configured maximum number of tokens per customer.
    #[must_use]
    pub const fn max_tokens_per_customer(&self) -> usize {
        self.max_tokens_per_customer
    }
}

fn insert_bounded_token(
    map: &mut BTreeMap<String, Vec<TokenId>>,
    customer_id: &str,
    token_id: TokenId,
    max_customers: usize,
    max_tokens_per_customer: usize,
) -> Result<(), MoyasarError> {
    if !map.contains_key(customer_id) && map.len() >= max_customers {
        return Err(MoyasarError::invalid_request(
            "customer vault capacity limit exceeded",
        ));
    }
    let tokens = map.entry(customer_id.to_owned()).or_default();
    if tokens.contains(&token_id) {
        return Ok(());
    }
    if tokens.len() >= max_tokens_per_customer {
        return Err(MoyasarError::invalid_request(
            "customer token capacity limit exceeded",
        ));
    }
    tokens.push(token_id);
    Ok(())
}

impl CustomerVault for InMemoryCustomerVault {
    fn associate<'a>(
        &'a self,
        customer_id: &'a str,
        token_id: TokenId,
    ) -> Pin<Box<dyn Future<Output = Result<(), MoyasarError>> + Send + 'a>> {
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        let res = insert_bounded_token(
            &mut guard,
            customer_id,
            token_id,
            self.max_customers,
            self.max_tokens_per_customer,
        );
        Box::pin(ready(res))
    }

    fn resolve_tokens<'a>(
        &'a self,
        customer_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<TokenId>, MoyasarError>> + Send + 'a>> {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        let tokens = guard.get(customer_id).cloned().unwrap_or_default();
        Box::pin(ready(Ok(tokens)))
    }

    fn deactivate<'a>(
        &'a self,
        token_id: &'a TokenId,
    ) -> Pin<Box<dyn Future<Output = Result<(), MoyasarError>> + Send + 'a>> {
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        for tokens in guard.values_mut() {
            tokens.retain(|t| t != token_id);
        }
        Box::pin(ready(Ok(())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn tc_8_1_0_03_associate_resolve_and_deactivate_roundtrip() {
        let vault = InMemoryCustomerVault::new();
        assert_eq!(vault.max_customers(), DEFAULT_MAX_CUSTOMERS);
        assert_eq!(
            vault.max_tokens_per_customer(),
            DEFAULT_MAX_TOKENS_PER_CUSTOMER
        );
        let token = TokenId::new("token_abc").expect("valid token");
        vault
            .associate("cus_101", token.clone())
            .await
            .expect("associate");
        vault
            .associate("cus_101", token.clone())
            .await
            .expect("idempotent");

        let resolved = vault.resolve_tokens("cus_101").await.expect("resolve");
        assert_eq!(resolved, vec![token.clone()]);

        vault.deactivate(&token).await.expect("deactivate");
        assert!(
            vault
                .resolve_tokens("cus_101")
                .await
                .expect("resolve")
                .is_empty()
        );
    }
}
