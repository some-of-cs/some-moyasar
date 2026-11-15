//! Managed Rocket application state holding Moyasar client and webhook guards.

use some_moyasar::prelude::{
    ConstantTimeSecretVerifier, InMemoryDeduplicator, MoyasarClient, SecretKey,
};

/// Shared Rocket application state for Moyasar API operations and webhook verification.
///
/// Register an instance via `rocket::build().manage(state)` and inject it into
/// route handlers as `&rocket::State<RocketAppState>`.
#[derive(Debug)]
pub struct RocketAppState {
    /// Authenticated Moyasar API client configured with a secret key.
    pub client: MoyasarClient<SecretKey>,
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
        Self {
            client,
            webhook_verifier,
            deduplicator,
        }
    }
}
