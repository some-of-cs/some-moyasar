//! Shared Axum application state for the `some-moyasar` reference server.
#![forbid(unsafe_code)]

use std::sync::Arc;

use some_moyasar::prelude::{
    ConstantTimeSecretVerifier, InMemoryCustomerVault, InMemoryDeduplicator, MoyasarClient,
    MoyasarError, PublishableKey, SecretKey,
};

/// Shared application state injected into Axum route handlers via `State<AppState>`.
#[derive(Clone)]
pub struct AppState {
    /// Thread-safe Moyasar API client authenticated with a secret key.
    pub client: MoyasarClient<SecretKey>,
    /// Thread-safe Moyasar API client authenticated with a publishable key (for Apple Pay / client-side flows).
    pub publishable_client: MoyasarClient<PublishableKey>,
    /// Constant-time webhook secret verifier with replay-window enforcement.
    pub verifier: Arc<ConstantTimeSecretVerifier>,
    /// In-memory idempotency store for deduplicating inbound webhook deliveries.
    pub deduplicator: Arc<InMemoryDeduplicator>,
    /// In-memory customer token vault for Stripe-compatible customer & subscription services.
    pub vault: Arc<InMemoryCustomerVault>,
}

impl AppState {
    /// Creates a new [`AppState`] from an explicit client, webhook verifier, and deduplicator.
    #[must_use]
    pub fn new(
        client: MoyasarClient<SecretKey>,
        verifier: Arc<ConstantTimeSecretVerifier>,
        deduplicator: Arc<InMemoryDeduplicator>,
    ) -> Self {
        let pk = PublishableKey::new("pk_test_axum_key").expect("valid default publishable key");
        let publishable_client = MoyasarClient::publishable_builder(pk)
            .base_url(client.base_url())
            .build()
            .expect("publishable client builds");
        Self {
            client,
            publishable_client,
            verifier,
            deduplicator,
            vault: Arc::new(InMemoryCustomerVault::new()),
        }
    }

    /// Builds an [`AppState`] from environment variables.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError`] if `MOYASAR_SECRET_KEY` is missing, malformed, or fails
    /// client initialization.
    #[allow(dead_code)]
    pub fn from_env() -> Result<Self, MoyasarError> {
        let secret_key = SecretKey::new("$MOYASAR_SECRET_KEY")?;
        let client = MoyasarClient::new(secret_key)?;
        let verifier = Arc::new(ConstantTimeSecretVerifier::with_default_tolerance(
            "$MOYASAR_WEBHOOK_SECRET",
        ));
        let deduplicator = Arc::new(InMemoryDeduplicator::new());
        Ok(Self::new(client, verifier, deduplicator))
    }
}
