//! Deterministic JSON fixtures and hermetic [`mock::MoyasarMockServer`] for offline testing.

/// Deterministic Moyasar payment and webhook JSON fixtures.
pub mod fixtures;
/// Hermetic wiremock-backed `MoyasarMockServer` for offline tests.
pub mod mock;

pub use fixtures::{
    ALL_WEBHOOK_EVENT_STRINGS, SADAD_SANDBOX_BILLER, STCPAY_SANDBOX_INSUFFICIENT_FUNDS,
    STCPAY_SANDBOX_REJECTED, STCPAY_SANDBOX_SUCCESS, all_webhook_event_fixtures,
    iso8583_error_fixture, sadad_sandbox_fixture, stcpay_sandbox_fixture, webhook_event_fixture,
};
pub use mock::MoyasarMockServer;
