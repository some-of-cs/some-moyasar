//! Async [`WebhookDeduplicator`] trait and in-memory/noop implementations (US-7.3.0 Unit 4).
//!
//! Provides an idempotency boundary keyed by `event.id` ([`Uuid`]) so duplicate
//! Moyasar webhook deliveries within the replay window do not trigger repeated
//! downstream side effects. Production deployments can implement
//! [`WebhookDeduplicator`] backed by Redis (`SET NX EX`) or PostgreSQL
//! (`INSERT ... ON CONFLICT DO NOTHING`).

use std::collections::HashMap;
use std::future::{Future, ready};
use std::pin::Pin;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use uuid::Uuid;

use super::event_types::WebhookError;
use super::timestamp::REPLAY_TOLERANCE;

/// Default maximum number of webhook event IDs retained in [`InMemoryDeduplicator`].
pub const DEFAULT_MAX_DEDUP_ENTRIES: usize = 100_000;

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

    /// Removes `event_id` from the deduplication store (for example, to roll back
    /// a reservation when downstream event processing fails so Moyasar's retry
    /// can be re-processed).
    fn remove(
        &self,
        _event_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<(), WebhookError>> + Send + '_>> {
        Box::pin(ready(Ok(())))
    }
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

/// Thread-safe in-memory [`WebhookDeduplicator`] with TTL expiration and bounded capacity.
///
/// Keyed by `event.id` ([`Uuid`]) to guarantee that concurrent webhook deliveries
/// for the same event ID result in at most one `Ok(true)` outcome while bounding
/// memory consumption under high-volume unique webhook replay floods.
#[derive(Debug)]
pub struct InMemoryDeduplicator {
    seen: Mutex<HashMap<Uuid, Instant>>,
    ttl: Duration,
    max_entries: usize,
}

impl Default for InMemoryDeduplicator {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryDeduplicator {
    /// Creates an empty [`InMemoryDeduplicator`] with [`REPLAY_TOLERANCE`] TTL and
    /// [`DEFAULT_MAX_DEDUP_ENTRIES`] capacity.
    ///
    /// # Examples
    ///
    /// ```
    /// use uuid::Uuid;
    /// use some_moyasar::webhook_engine::deduplication::{
    ///     InMemoryDeduplicator, WebhookDeduplicator,
    /// };
    ///
    /// let dedup = InMemoryDeduplicator::new();
    /// let event_id = Uuid::nil();
    /// assert!(futures::executor::block_on(dedup.check_and_record(event_id)).unwrap());
    /// assert!(!futures::executor::block_on(dedup.check_and_record(event_id)).unwrap());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity_limit(REPLAY_TOLERANCE, DEFAULT_MAX_DEDUP_ENTRIES)
    }

    /// Creates an [`InMemoryDeduplicator`] with an explicit entry `ttl` and `max_entries` cap.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use uuid::Uuid;
    /// use some_moyasar::webhook_engine::deduplication::{
    ///     InMemoryDeduplicator, WebhookDeduplicator,
    /// };
    ///
    /// let dedup = InMemoryDeduplicator::with_capacity_limit(Duration::from_secs(60), 2);
    /// assert_eq!(dedup.max_entries(), 2);
    /// assert!(futures::executor::block_on(dedup.check_and_record(Uuid::new_v4())).unwrap());
    /// ```
    #[must_use]
    pub fn with_capacity_limit(ttl: Duration, max_entries: usize) -> Self {
        Self {
            seen: Mutex::new(HashMap::new()),
            ttl,
            max_entries: max_entries.max(1),
        }
    }

    /// Returns the configured maximum number of retained event IDs.
    #[must_use]
    pub const fn max_entries(&self) -> usize {
        self.max_entries
    }

    /// Returns the configured TTL duration for retained event IDs.
    #[must_use]
    pub const fn ttl(&self) -> Duration {
        self.ttl
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

    /// Clears all recorded event IDs from the in-memory store.
    pub fn clear(&self) {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }
}

fn purge_expired_locked(seen: &mut HashMap<Uuid, Instant>, now: Instant, ttl: Duration) {
    seen.retain(|_, &mut recorded_at| now.saturating_duration_since(recorded_at) < ttl);
}

fn evict_oldest_locked(seen: &mut HashMap<Uuid, Instant>) {
    if let Some(oldest_id) = seen.iter().min_by_key(|(_, ts)| *ts).map(|(id, _)| *id) {
        seen.remove(&oldest_id);
    }
}

impl WebhookDeduplicator for InMemoryDeduplicator {
    fn check_and_record(
        &self,
        event_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<bool, WebhookError>> + Send + '_>> {
        let is_fresh = {
            let now = Instant::now();
            let mut guard = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
            purge_expired_locked(&mut guard, now, self.ttl);
            if guard.contains_key(&event_id) {
                false
            } else {
                if guard.len() >= self.max_entries {
                    evict_oldest_locked(&mut guard);
                }
                guard.insert(event_id, now);
                true
            }
        };
        Box::pin(ready(Ok(is_fresh)))
    }

    fn remove(
        &self,
        event_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = Result<(), WebhookError>> + Send + '_>> {
        {
            let mut guard = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
            guard.remove(&event_id);
        }
        Box::pin(ready(Ok(())))
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
        deduplicator.remove(event_id).await.unwrap();
    }

    #[tokio::test]
    async fn in_memory_deduplicator_accepts_first_rejects_duplicate_and_supports_remove_and_clear()
    {
        let deduplicator = InMemoryDeduplicator::new();
        assert!(deduplicator.is_empty());

        let event_id = Uuid::new_v4();
        let other_id = Uuid::new_v4();

        assert!(deduplicator.check_and_record(event_id).await.unwrap());
        assert!(!deduplicator.check_and_record(event_id).await.unwrap());
        assert!(deduplicator.check_and_record(other_id).await.unwrap());
        assert_eq!(deduplicator.len(), 2);
        assert!(!deduplicator.is_empty());

        deduplicator.remove(event_id).await.unwrap();
        assert_eq!(deduplicator.len(), 1);
        assert!(deduplicator.check_and_record(event_id).await.unwrap());

        deduplicator.clear();
        assert!(deduplicator.is_empty());
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

    #[tokio::test]
    async fn in_memory_deduplicator_evicts_oldest_when_capacity_exceeded() {
        let dedup = InMemoryDeduplicator::with_capacity_limit(Duration::from_secs(60), 3);
        let id1 = Uuid::from_u128(1);
        let id2 = Uuid::from_u128(2);
        let id3 = Uuid::from_u128(3);
        let id4 = Uuid::from_u128(4);

        assert!(dedup.check_and_record(id1).await.unwrap());
        assert!(dedup.check_and_record(id2).await.unwrap());
        assert!(dedup.check_and_record(id3).await.unwrap());
        assert_eq!(dedup.len(), 3);

        assert!(dedup.check_and_record(id4).await.unwrap());
        assert_eq!(dedup.len(), 3);
        assert!(dedup.check_and_record(id1).await.unwrap());
    }
}
