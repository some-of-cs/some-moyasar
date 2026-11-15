//! Shared Actix-Web application state for `some-moyasar`.

use std::sync::Arc;

use some_moyasar::prelude::{
    ConstantTimeSecretVerifier, InMemoryCustomerVault, InMemoryDeduplicator, MoyasarClient,
    SecretKey,
};

/// Shared, thread-safe Actix-Web application state for `some-moyasar` handlers.
#[derive(Clone)]
pub struct ActixAppState {
    /// Authenticated Moyasar API client configured with a secret key.
    pub client: Arc<MoyasarClient<SecretKey>>,
    /// In-memory customer-to-token vault for Stripe-compatible customer operations.
    pub vault: Arc<InMemoryCustomerVault>,
    /// Constant-time HMAC-SHA256 webhook secret verifier.
    pub verifier: Arc<ConstantTimeSecretVerifier>,
    /// In-memory webhook event deduplicator for idempotent webhook handling.
    pub deduplicator: Arc<InMemoryDeduplicator>,
}

impl ActixAppState {
    /// Creates a new [`ActixAppState`] with default in-memory vault, deduplicator,
    /// and replay-tolerant webhook secret verifier.
    #[must_use]
    pub fn new(client: MoyasarClient<SecretKey>, webhook_secret: &str) -> Self {
        Self {
            client: Arc::new(client),
            vault: Arc::new(InMemoryCustomerVault::new()),
            verifier: Arc::new(ConstantTimeSecretVerifier::with_default_tolerance(
                webhook_secret,
            )),
            deduplicator: Arc::new(InMemoryDeduplicator::new()),
        }
    }
}
