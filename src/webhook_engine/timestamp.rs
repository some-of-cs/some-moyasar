//! Webhook replay window and future-skew timestamp validation (US-7.3.0 Unit 3).

use std::time::Duration;

use chrono::{DateTime, Utc};

use super::event_types::{WebhookError, WebhookEvent};

/// Default replay tolerance window (4 hours) covering Moyasar's 6-attempt
/// 3h41m webhook retry ladder (`0m`, `1m`, `5m`, `15m`, `60m`, `221m`).
pub const REPLAY_TOLERANCE: Duration = Duration::from_secs(4 * 60 * 60);

/// Maximum allowed clock skew in the future (5 minutes) for inbound webhook
/// `created_at` timestamps.
pub const MAX_FUTURE_SKEW: Duration = Duration::from_secs(5 * 60);

/// Validates that a webhook `created_at` timestamp falls within `[now - tolerance, now + MAX_FUTURE_SKEW]`.
///
/// # Errors
///
/// Returns [`WebhookError::TimestampOutOfRange`] if `created_at` is older than
/// `tolerance` relative to `now`, or if `created_at` is further in the future
/// than [`MAX_FUTURE_SKEW`].
pub fn validate_timestamp_at(
    created_at: DateTime<Utc>,
    now: DateTime<Utc>,
    tolerance: Duration,
) -> Result<(), WebhookError> {
    if created_at > now {
        let skew = (created_at - now)
            .to_std()
            .map_err(|_| WebhookError::TimestampOutOfRange)?;
        if skew > MAX_FUTURE_SKEW {
            return Err(WebhookError::TimestampOutOfRange);
        }
        return Ok(());
    }

    let age = (now - created_at)
        .to_std()
        .map_err(|_| WebhookError::TimestampOutOfRange)?;
    if age > tolerance {
        return Err(WebhookError::TimestampOutOfRange);
    }

    Ok(())
}

impl WebhookEvent {
    /// Validates that this event's `created_at` timestamp falls within the
    /// replay `tolerance` window and [`MAX_FUTURE_SKEW`] relative to `now`.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookError::TimestampOutOfRange`] if `self.created_at` is
    /// older than `tolerance` relative to `now`, or if it exceeds
    /// [`MAX_FUTURE_SKEW`] in the future.
    pub fn validate_timestamp_at(
        &self,
        now: DateTime<Utc>,
        tolerance: Duration,
    ) -> Result<(), WebhookError> {
        validate_timestamp_at(self.created_at, now, tolerance)
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeDelta;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;
    use crate::webhook_engine::event_types::WebhookEventType;

    fn sample_event(created_at: DateTime<Utc>) -> WebhookEvent {
        WebhookEvent {
            id: Uuid::nil(),
            event_type: WebhookEventType::PaymentPaid,
            created_at,
            secret_token: "whsec_test_secret".to_owned(),
            account_name: Some("test_account".to_owned()),
            live: false,
            data: json!({ "id": "pay_test_123" }),
        }
    }

    #[test]
    fn tc_7_3_0_03_stale_timestamp_beyond_4h_rejected() {
        let now = DateTime::from_timestamp(1_700_000_000, 0).expect("valid timestamp");
        let stale = now - TimeDelta::seconds(4 * 60 * 60 + 1);
        let result = validate_timestamp_at(stale, now, REPLAY_TOLERANCE);
        assert!(matches!(result, Err(WebhookError::TimestampOutOfRange)));

        let boundary = now - TimeDelta::seconds(4 * 60 * 60);
        assert!(validate_timestamp_at(boundary, now, REPLAY_TOLERANCE).is_ok());
    }

    #[test]
    fn tc_7_3_0_03_future_skew_beyond_5m_rejected() {
        let now = DateTime::from_timestamp(1_700_000_000, 0).expect("valid timestamp");
        let too_far_future = now + TimeDelta::seconds(5 * 60 + 1);
        let result = validate_timestamp_at(too_far_future, now, REPLAY_TOLERANCE);
        assert!(matches!(result, Err(WebhookError::TimestampOutOfRange)));

        let boundary = now + TimeDelta::seconds(5 * 60);
        assert!(validate_timestamp_at(boundary, now, REPLAY_TOLERANCE).is_ok());
    }

    #[test]
    fn tc_7_3_0_04_retry_ladder_accepted() {
        let created_at = DateTime::from_timestamp(1_700_000_000, 0).expect("valid timestamp");
        let event = sample_event(created_at);
        let retry_offsets_minutes = [0_i64, 1, 5, 15, 60, 221];

        for offset_min in retry_offsets_minutes {
            let now = created_at + TimeDelta::minutes(offset_min);
            assert!(
                event.validate_timestamp_at(now, REPLAY_TOLERANCE).is_ok(),
                "retry at +{offset_min}m should be accepted within 4h tolerance"
            );
        }
    }
}
