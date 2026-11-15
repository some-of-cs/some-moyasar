//! Managed Rocket application state holding Moyasar clients, vault, and webhook guards.

use some_moyasar::prelude::{
    ConstantTimeSecretVerifier, InMemoryCustomerVault, InMemoryDeduplicator, MoyasarClient,
    PublishableKey, SecretKey,
};

/// Shared Rocket application state for Moyasar API operations and webhook verification.
///
/// Register an instance via `rocket::build().manage(state)` and inject it into
/// route handlers as `&rocket::State<RocketAppState>`.
#[derive(Debug)]
pub struct RocketAppState {
    /// Authenticated Moyasar API client configured with a secret key.
    pub client: MoyasarClient<SecretKey>,
    /// Publishable Moyasar API client for Apple Pay session initiation and BIN lookup.
    pub publishable_client: MoyasarClient<PublishableKey>,
    /// In-memory customer token vault for Stripe-compatible customer charges.
    pub customer_vault: InMemoryCustomerVault,
    /// Constant-time Double-HMAC verifier for inbound webhook secret tokens.
    pub webhook_verifier: ConstantTimeSecretVerifier,
    /// Thread-safe in-memory deduplicator for idempotent webhook processing.
    pub deduplicator: InMemoryDeduplicator,
}

impl RocketAppState {
    /// Creates a new [`RocketAppState`] from an authenticated client, webhook verifier, and deduplicator.
    #[must_use]
    pub fn new(
        client: MoyasarClient<SecretKey>,
        webhook_verifier: ConstantTimeSecretVerifier,
        deduplicator: InMemoryDeduplicator,
    ) -> Self {
        let publishable_client =
            MoyasarClient::publishable_builder("pk_test_rocket_default_key")
                .base_url(client.base_url())
                .timeout(client.timeout())
                .max_network_retries(client.max_network_retries())
                .build()
                .expect("publishable client should build from validated secret client config");
        Self {
            client,
            publishable_client,
            customer_vault: InMemoryCustomerVault::new(),
            webhook_verifier,
            deduplicator,
        }
    }
}
