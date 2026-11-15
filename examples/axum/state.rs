//! Shared Axum application state for the `some-moyasar` reference server.
#![forbid(unsafe_code)]

use std::sync::Arc;

use some_moyasar::prelude::{
    ConstantTimeSecretVerifier, InMemoryDeduplicator, MoyasarClient, MoyasarError, SecretKey,
};

/// Shared application state injected into Axum route handlers via `State<AppState>`.
#[derive(Clone)]
pub struct AppState {
    /// Thread-safe Moyasar API client authenticated with a secret key.
    pub client: MoyasarClient<SecretKey>,
    /// Constant-time webhook secret verifier with replay-window enforcement.
    pub verifier: Arc<ConstantTimeSecretVerifier>,
    /// In-memory idempotency store for deduplicating inbound webhook deliveries.
    pub deduplicator: Arc<InMemoryDeduplicator>,
}

impl AppState {
    /// Creates a new [`AppState`] from an explicit client, webhook verifier, and deduplicator.
    #[must_use]
    pub fn new(
        client: MoyasarClient<SecretKey>,
        verifier: Arc<ConstantTimeSecretVerifier>,
        deduplicator: Arc<InMemoryDeduplicator>,
    ) -> Self {
        Self {
            client,
            verifier,
            deduplicator,
        }
    }

    /// Builds an [`AppState`] from the `MOYASAR_SECRET_KEY` and `MOYASAR_WEBHOOK_SECRET`
    /// environment variables.
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
