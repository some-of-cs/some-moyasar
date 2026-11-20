//! Bounded exponential backoff with deterministic jitter and `Retry-After` parsing.

use std::time::Duration;

use http::HeaderMap;
use http::header::RETRY_AFTER;

/// Default initial backoff delay before the first retry (`50 ms`).
pub const DEFAULT_BASE_DELAY: Duration = Duration::from_millis(50);

/// Default upper cap on computed backoff delays (`2 s`).
pub const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(2);

/// Default deterministic jitter ceiling in milliseconds (`25 ms`).
pub const DEFAULT_MAX_JITTER_MS: u64 = 25;

/// Maximum `Retry-After` delay honored from server responses (`60 s`).
pub const MAX_RETRY_AFTER_SECS: u64 = 60;

/// Runtime-agnostic bounded exponential backoff configuration with deterministic jitter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackoffPolicy {
    /// Initial delay before the first retry (`attempt == 0`).
    pub base_delay: Duration,
    /// Upper bound on any computed retry delay.
    pub max_delay: Duration,
    /// Maximum deterministic jitter in milliseconds (`0` disables jitter).
    pub max_jitter_ms: u64,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            base_delay: DEFAULT_BASE_DELAY,
            max_delay: DEFAULT_MAX_DELAY,
            max_jitter_ms: DEFAULT_MAX_JITTER_MS,
        }
    }
}

impl BackoffPolicy {
    /// Creates a custom bounded backoff policy.
    #[must_use]
    pub const fn new(base_delay: Duration, max_delay: Duration, max_jitter_ms: u64) -> Self {
        Self {
            base_delay,
            max_delay,
            max_jitter_ms,
        }
    }

    /// Creates a zero-delay policy useful for immediate retries in tests.
    #[must_use]
    pub const fn no_delay() -> Self {
        Self {
            base_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
            max_jitter_ms: 0,
        }
    }

    /// Computes the bounded exponential delay with deterministic jitter for `attempt` (`0`-based).
    #[must_use]
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let base_ms = u64::try_from(self.base_delay.as_millis()).unwrap_or(u64::MAX);
        let max_ms = u64::try_from(self.max_delay.as_millis()).unwrap_or(u64::MAX);
        let factor = 1_u64 << attempt.min(31);
        let exp_ms = base_ms.saturating_mul(factor);
        let jitter_ms = deterministic_jitter_ms(attempt, self.max_jitter_ms);
        Duration::from_millis(exp_ms.saturating_add(jitter_ms).min(max_ms))
    }

    /// Computes the retry delay, preferring a valid numeric `Retry-After` header capped at
    /// `self.max_delay` and falling back to [`Self::delay_for_attempt`].
    #[must_use]
    pub fn delay_with_retry_after(&self, attempt: u32, headers: &HeaderMap) -> Duration {
        parse_retry_after_secs(headers)
            .map(|retry_after| retry_after.min(self.max_delay))
            .unwrap_or_else(|| self.delay_for_attempt(attempt))
    }
}

/// Computes bounded exponential backoff with deterministic jitter using [`BackoffPolicy::default`].
///
/// Starts at `50 ms` (`attempt == 0`), doubles per attempt, adds deterministic jitter,
/// and caps at `2 s`.
#[must_use]
pub fn backoff_duration(attempt: u32) -> Duration {
    BackoffPolicy::default().delay_for_attempt(attempt)
}

/// Parses a numeric `Retry-After` HTTP header value (in seconds) into a bounded [`Duration`].
///
/// Missing headers or non-numeric values return `None`. Parsed durations are capped at
/// [`MAX_RETRY_AFTER_SECS`] to prevent unbounded client stalls.
#[must_use]
pub fn parse_retry_after_secs(headers: &HeaderMap) -> Option<Duration> {
    let raw = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    let secs = raw.parse::<u64>().ok()?;
    Some(Duration::from_secs(secs.min(MAX_RETRY_AFTER_SECS)))
}

const fn deterministic_jitter_ms(attempt: u32, max_jitter_ms: u64) -> u64 {
    if max_jitter_ms == 0 {
        return 0;
    }
    let mixed = (attempt as u64)
        .wrapping_add(1)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let spread = mixed ^ (mixed >> 32);
    match max_jitter_ms.checked_add(1) {
        Some(bound) => spread % bound,
        None => spread,
    }
}

#[cfg(test)]
mod tests {
    use http::HeaderValue;

    use super::*;

    #[test]
    fn backoff_duration_doubles_and_caps_at_two_seconds() {
        let d0 = backoff_duration(0);
        let d1 = backoff_duration(1);
        let d2 = backoff_duration(2);
        let d10 = backoff_duration(10);
        let d100 = backoff_duration(100);

        assert!(d0 >= Duration::from_millis(50) && d0 <= Duration::from_millis(75));
        assert!(d1 >= Duration::from_millis(100) && d1 <= Duration::from_millis(125));
        assert!(d2 >= Duration::from_millis(200) && d2 <= Duration::from_millis(225));
        assert_eq!(d10, DEFAULT_MAX_DELAY);
        assert_eq!(d100, DEFAULT_MAX_DELAY);
    }

    #[test]
    fn backoff_policy_is_deterministic_and_supports_zero_jitter() {
        let default_policy = BackoffPolicy::default();
        for attempt in 0..8 {
            assert_eq!(
                default_policy.delay_for_attempt(attempt),
                default_policy.delay_for_attempt(attempt)
            );
        }

        let no_jitter = BackoffPolicy::new(Duration::from_millis(50), Duration::from_secs(2), 0);
        assert_eq!(no_jitter.delay_for_attempt(0), Duration::from_millis(50));
        assert_eq!(no_jitter.delay_for_attempt(1), Duration::from_millis(100));
        assert_eq!(no_jitter.delay_for_attempt(2), Duration::from_millis(200));
        assert_eq!(no_jitter.delay_for_attempt(3), Duration::from_millis(400));
        assert_eq!(no_jitter.delay_for_attempt(4), Duration::from_millis(800));
        assert_eq!(no_jitter.delay_for_attempt(5), Duration::from_millis(1_600));
        assert_eq!(no_jitter.delay_for_attempt(6), Duration::from_secs(2));
        assert_eq!(
            BackoffPolicy::no_delay().delay_for_attempt(3),
            Duration::ZERO
        );
    }

    #[test]
    fn parse_retry_after_secs_parses_caps_and_rejects_invalid() {
        let empty = HeaderMap::new();
        assert_eq!(parse_retry_after_secs(&empty), None);

        let mut valid = HeaderMap::new();
        valid.insert(RETRY_AFTER, HeaderValue::from_static(" 3 "));
        assert_eq!(parse_retry_after_secs(&valid), Some(Duration::from_secs(3)));

        let mut huge = HeaderMap::new();
        huge.insert(RETRY_AFTER, HeaderValue::from_static("9999"));
        assert_eq!(
            parse_retry_after_secs(&huge),
            Some(Duration::from_secs(MAX_RETRY_AFTER_SECS))
        );

        for invalid_val in ["Wed, 21 Oct 2026 07:28:00 GMT", "-5", "abc", ""] {
            let mut headers = HeaderMap::new();
            headers.insert(RETRY_AFTER, HeaderValue::from_static(invalid_val));
            assert_eq!(parse_retry_after_secs(&headers), None);
        }
    }

    #[test]
    fn delay_with_retry_after_prefers_header_and_caps_by_policy() {
        let policy = BackoffPolicy::new(Duration::from_millis(50), Duration::from_secs(2), 0);
        let empty = HeaderMap::new();
        assert_eq!(
            policy.delay_with_retry_after(1, &empty),
            Duration::from_millis(100)
        );

        let mut short = HeaderMap::new();
        short.insert(RETRY_AFTER, HeaderValue::from_static("1"));
        assert_eq!(
            policy.delay_with_retry_after(0, &short),
            Duration::from_secs(1)
        );

        let mut long = HeaderMap::new();
        long.insert(RETRY_AFTER, HeaderValue::from_static("10"));
        assert_eq!(
            policy.delay_with_retry_after(0, &long),
            Duration::from_secs(2)
        );
    }
}
