//! Async [`WebhookDeduplicator`] trait and in-memory/noop implementations (US-7.3.0 Unit 4).
//!
//! Provides an idempotency boundary keyed by `event.id` ([`Uuid`]) so duplicate
//! Moyasar webhook deliveries within the replay window do not trigger repeated
//! downstream side effects. Production deployments can implement
//! [`WebhookDeduplicator`] backed by Redis (`SET NX EX`) or PostgreSQL
//! (`INSERT ... ON CONFLICT DO NOTHING`).

use std::collections::HashSet;
use std::future::{Future, ready};
use std::pin::Pin;
use std::sync::{Mutex, PoisonError};

use uuid::Uuid;

use super::verifier::WebhookError;

/// Asynchronous idempotency store for deduplicating inbound Moyasar webhook events.
///
/// Implementations must be `Send + Sync` so they can be shared across async HTTP
/// handler tasks (for example inside `Arc<dyn WebhookDeduplicator>`).
///
/// # Persistence Backends
///
/// While [`InMemoryDeduplicator`] is suitable for single-instance services and
/// tests, distributed deployments should provide a custom implementation backed
/// by Redis or PostgreSQL.
pub trait WebhookDeduplicator: Send + Sync {
    /// Checks whether `event_id` has been seen before and records it atomically.
    ///
    /// Returns `Ok(true)` if `event_id` is fresh and has been recorded, or
    /// `Ok(false)` if `event_id` was already recorded as a duplicate.
    fn check_and_record(
        &self,
        event_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<bool, WebhookError>> + Send + '_>>;
}

/// No-op [`WebhookDeduplicator`] that treats every webhook event as fresh.
///
/// Useful for stateless verification pipelines or local development where
/// downstream handlers are already idempotent.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopDeduplicator;

impl NoopDeduplicator {
    /// Creates a new [`NoopDeduplicator`].
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl WebhookDeduplicator for NoopDeduplicator {
    fn check_and_record(
        &self,
        _event_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<bool, WebhookError>> + Send + '_>> {
        Box::pin(ready(Ok(true)))
    }
}

/// Thread-safe in-memory [`WebhookDeduplicator`] backed by a `Mutex<HashSet<Uuid>>`.
///
/// Keyed by `event.id` ([`Uuid`]) to guarantee that concurrent webhook deliveries
/// for the same event ID result in at most one `Ok(true)` outcome.
#[derive(Debug, Default)]
pub struct InMemoryDeduplicator {
    seen: Mutex<HashSet<Uuid>>,
}

impl InMemoryDeduplicator {
    /// Creates an empty [`InMemoryDeduplicator`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            seen: Mutex::new(HashSet::new()),
        }
    }

    /// Returns the number of recorded event IDs currently stored.
    #[must_use]
    pub fn len(&self) -> usize {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Returns `true` if no event IDs have been recorded yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl WebhookDeduplicator for InMemoryDeduplicator {
    fn check_and_record(
        &self,
        event_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<bool, WebhookError>> + Send + '_>> {
        let is_fresh = {
            let mut guard = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
            guard.insert(event_id)
        };
        Box::pin(ready(Ok(is_fresh)))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[tokio::test]
    async fn noop_deduplicator_always_returns_true() {
        let deduplicator = NoopDeduplicator::new();
        let event_id = Uuid::new_v4();

        assert!(deduplicator.check_and_record(event_id).await.unwrap());
        assert!(deduplicator.check_and_record(event_id).await.unwrap());
    }

    #[tokio::test]
    async fn in_memory_deduplicator_accepts_first_and_rejects_duplicate() {
        let deduplicator = InMemoryDeduplicator::new();
        assert!(deduplicator.is_empty());

        let event_id = Uuid::new_v4();
        let other_id = Uuid::new_v4();

        assert!(deduplicator.check_and_record(event_id).await.unwrap());
        assert!(!deduplicator.check_and_record(event_id).await.unwrap());
        assert!(deduplicator.check_and_record(other_id).await.unwrap());
        assert_eq!(deduplicator.len(), 2);
        assert!(!deduplicator.is_empty());
    }

    #[tokio::test]
    async fn in_memory_deduplicator_allows_only_one_concurrent_winner() {
        let deduplicator = Arc::new(InMemoryDeduplicator::new());
        let event_id = Uuid::new_v4();
        let task_count = 32;

        let mut handles = Vec::with_capacity(task_count);
        for _ in 0..task_count {
            let dedup = Arc::clone(&deduplicator);
            handles.push(tokio::spawn(async move {
                dedup.check_and_record(event_id).await.unwrap()
            }));
        }

        let mut fresh_count = 0usize;
        for handle in handles {
            if handle.await.unwrap() {
                fresh_count += 1;
            }
        }

        assert_eq!(fresh_count, 1);
        assert_eq!(deduplicator.len(), 1);
    }
}
