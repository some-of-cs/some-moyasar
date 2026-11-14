//! Deterministic JSON fixtures and hermetic [`mock::MoyasarMockServer`] for offline testing.

/// Deterministic Moyasar payment and webhook JSON fixtures.
pub mod fixtures;
/// Hermetic wiremock-backed `MoyasarMockServer` for offline tests.
pub mod mock;
