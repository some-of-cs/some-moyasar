//! `some-payment` orchestrator alignment rules, pre-flight guards, and webhook helpers.

/// Checkout intent validation and alignment helpers.
pub mod checkout_intents;
/// Idempotency key normalization and error mapping helpers.
pub mod idempotency_errors;
/// Key-prefix mode detection, local scope guards, and injectable HTTP configuration.
pub mod key_scope;
/// PAN redaction and cross-gateway payment status mapping.
pub mod pan_and_status;
/// Pre-flight validation rules for UUIDs, manual capture, metadata, and splits.
pub mod preflight_rules;
/// Live webhook event lookup table and verification handshake helpers.
#[cfg(feature = "webhook-engine")]
pub mod webhook_handshake;
/// Constant-time webhook token verification helpers.
#[cfg(feature = "webhook-engine")]
pub mod webhook_token_verifier;
