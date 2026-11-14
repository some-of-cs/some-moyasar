//! Constant-time webhook verification engine, replay window, deduplication, and typed extractors.

pub mod deduplication;
pub mod event_types;
pub mod payload;
pub mod timestamp;
pub mod verifier;

pub use deduplication::{InMemoryDeduplicator, NoopDeduplicator, WebhookDeduplicator};
pub use event_types::{WebhookError, WebhookEvent, WebhookEventType};
pub use timestamp::{MAX_FUTURE_SKEW, REPLAY_TOLERANCE, validate_timestamp_at};
pub use verifier::{ConstantTimeSecretVerifier, DOMAIN_SEPARATION_KEY};
