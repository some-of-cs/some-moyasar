//! Shared Actix-Web application state for `some-moyasar`.

use std::sync::Arc;

use some_moyasar::prelude::{
    ConstantTimeSecretVerifier, InMemoryCustomerVault, InMemoryDeduplicator, MoyasarClient,
    PublishableKey, SecretKey,
};

/// Shared, thread-safe Actix-Web application state for `some-moyasar` handlers.
#[derive(Clone)]
pub struct ActixAppState {
    /// Authenticated Moyasar API client configured with a secret key.
    pub client: Arc<MoyasarClient<SecretKey>>,
    /// Moyasar API client configured with a publishable key for storefront/Apple Pay routes.
    pub pub_client: Arc<MoyasarClient<PublishableKey>>,
    /// In-memory customer-to-token vault for Stripe-compatible customer operations.
    pub vault: Arc<InMemoryCustomerVault>,
    /// Constant-time HMAC-SHA256 webhook secret verifier.
    pub verifier: Arc<ConstantTimeSecretVerifier>,
    /// In-memory webhook event deduplicator for idempotent webhook handling.
    pub deduplicator: Arc<InMemoryDeduplicator>,
}

impl ActixAppState {
    /// Creates a new [`ActixAppState`] with default in-memory vault, deduplicator,
    /// publishable client sharing the secret client's `base_url`, and webhook verifier.
    #[must_use]
    pub fn new(client: MoyasarClient<SecretKey>, webhook_secret: &str) -> Self {
        let pub_key = PublishableKey::new("pk_test_actix_default")
            .expect("static publishable key is valid");
        let pub_client = MoyasarClient::builder(pub_key)
            .base_url(client.config().base_url.clone())
            .build()
            .expect("publishable client shares validated base_url");
        Self {
            client: Arc::new(client),
            pub_client: Arc::new(pub_client),
            vault: Arc::new(InMemoryCustomerVault::new()),
            verifier: Arc::new(ConstantTimeSecretVerifier::with_default_tolerance(
                webhook_secret,
            )),
            deduplicator: Arc::new(InMemoryDeduplicator::new()),
        }
    }

    /// Replaces the publishable-key [`MoyasarClient`] used by storefront and Apple Pay routes.
    #[must_use]
    pub fn with_publishable_client(mut self, pub_client: MoyasarClient<PublishableKey>) -> Self {
        self.pub_client = Arc::new(pub_client);
        self
    }
}
