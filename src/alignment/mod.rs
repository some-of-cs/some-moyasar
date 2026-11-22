//! `some-payment` orchestrator alignment rules, pre-flight guards, and webhook helpers.

pub mod checkout_intents;
pub mod idempotency_errors;
pub mod key_scope;
pub mod pan_and_status;
pub mod preflight_rules;
#[cfg(feature = "webhook-engine")]
pub mod webhook_handshake;
#[cfg(feature = "webhook-engine")]
pub mod webhook_token_verifier;
