//! Constant-time webhook verification engine, replay window, deduplication, and typed extractors.

/// Async webhook event deduplication trait and in-memory/noop stores.
pub mod deduplication;
/// Canonical Moyasar webhook event types, payloads, and error variants.
pub mod event_types;
/// Typed payload extractors for verified Moyasar webhook events.
pub mod payload;
/// Replay-window and future-skew timestamp validation for webhooks.
pub mod timestamp;
/// Constant-time Double-HMAC webhook secret and signature verifiers.
pub mod verifier;

pub use deduplication::{
    DEFAULT_MAX_DEDUP_ENTRIES, InMemoryDeduplicator, NoopDeduplicator, WebhookDeduplicator,
};
pub use event_types::{WebhookError, WebhookEvent, WebhookEventType};
pub use timestamp::{MAX_FUTURE_SKEW, REPLAY_TOLERANCE, validate_timestamp_at};
pub use verifier::{ConstantTimeSecretVerifier, DOMAIN_SEPARATION_KEY, verify_signature};
