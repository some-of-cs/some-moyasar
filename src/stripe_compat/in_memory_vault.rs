//! Thread-safe `InMemoryCustomerVault` reference implementation.

use std::collections::BTreeMap;
use std::future::{Future, ready};
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use crate::kernel::error::MoyasarError;
use crate::resources::tokens::TokenId;
use crate::stripe_compat::customers::CustomerVault;

/// Thread-safe, in-memory [`CustomerVault`] backed by `Arc<RwLock<...>>`.
///
/// Suitable for single-instance services and hermetic unit tests. Production
/// distributed deployments can implement [`CustomerVault`] over PostgreSQL or Redis.
///
/// # Examples
///
/// ```
/// # tokio_test::block_on(async {
/// use some_moyasar::resources::tokens::TokenId;
/// use some_moyasar::stripe_compat::customers::CustomerVault;
/// use some_moyasar::stripe_compat::in_memory_vault::InMemoryCustomerVault;
///
/// let vault = InMemoryCustomerVault::new();
/// let token = TokenId::new("token_abc123").unwrap();
/// vault.associate("cus_101", token.clone()).await.unwrap();
/// assert_eq!(vault.resolve_tokens("cus_101").await.unwrap(), vec![token.clone()]);
/// vault.deactivate(&token).await.unwrap();
/// assert!(vault.resolve_tokens("cus_101").await.unwrap().is_empty());
/// # });
/// ```
#[derive(Debug, Clone, Default)]
pub struct InMemoryCustomerVault {
    inner: Arc<RwLock<BTreeMap<String, Vec<TokenId>>>>,
}

impl InMemoryCustomerVault {
    /// Creates an empty [`InMemoryCustomerVault`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl CustomerVault for InMemoryCustomerVault {
    fn associate<'a>(
        &'a self,
        customer_id: &'a str,
        token_id: TokenId,
    ) -> Pin<Box<dyn Future<Output = Result<(), MoyasarError>> + Send + 'a>> {
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        let tokens = guard.entry(customer_id.to_owned()).or_default();
        if !tokens.contains(&token_id) {
            tokens.push(token_id);
        }
        Box::pin(ready(Ok(())))
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
        let token = TokenId::new("token_abc").expect("valid token");
        vault.associate("cus_101", token.clone()).await.expect("associate");
        vault.associate("cus_101", token.clone()).await.expect("idempotent");

        let resolved = vault.resolve_tokens("cus_101").await.expect("resolve");
        assert_eq!(resolved, vec![token.clone()]);

        vault.deactivate(&token).await.expect("deactivate");
        assert!(vault.resolve_tokens("cus_101").await.expect("resolve").is_empty());
    }
}
