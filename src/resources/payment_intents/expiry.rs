//! Payment Intent 1-hour expiry guard (`is_usable`).
//!
//! # Silent-Status Pitfall
//!
//! Moyasar Payment Intents have a fixed, non-configurable 1-hour (`3600` seconds)
//! expiration window. Even after `expires_at` passes, the API continues to report
//! `status: "created"`. Always call [`PaymentIntent::is_usable`] before attempting
//! `cancel()` or browser `fulfill()` to avoid spurious `400 intent_not_cancelable`
//! or expired-intent errors.

use chrono::{DateTime, Utc};

use crate::resources::payment_intents::models::{PaymentIntent, PaymentIntentStatus};

impl PaymentIntent {
    /// Returns `true` only when `status == PaymentIntentStatus::Created` and `Utc::now() < expires_at`.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::Utc;
    /// use some_moyasar::resources::payment_intents::models::PaymentIntent;
    ///
    /// let intent: PaymentIntent = serde_json::from_str(r#"{
    ///     "id": "pi_doc_1",
    ///     "status": "created",
    ///     "amount": 1000,
    ///     "currency": "SAR",
    ///     "expires_at": "2020-01-01T00:00:00Z",
    ///     "created_at": "2019-12-31T23:00:00Z"
    /// }"#).unwrap();
    /// assert!(!intent.is_usable());
    /// ```
    pub fn is_usable(&self) -> bool {
        self.is_usable_at(Utc::now())
    }

    /// Evaluates usability against an explicit reference timestamp `now`.
    ///
    /// Returns `true` strictly when `self.status == PaymentIntentStatus::Created && now < self.expires_at`.
    pub fn is_usable_at(&self, now: DateTime<Utc>) -> bool {
        self.status == PaymentIntentStatus::Created && now < self.expires_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn sample_intent(status: &str, expires_at: DateTime<Utc>) -> PaymentIntent {
        let created_at = expires_at - Duration::seconds(3600);
        let json = format!(
            r#"{{
                "id": "pi_exp_1",
                "status": "{status}",
                "amount": 5000,
                "currency": "SAR",
                "expires_at": "{}",
                "created_at": "{}"
            }}"#,
            expires_at.to_rfc3339(),
            created_at.to_rfc3339()
        );
        serde_json::from_str(&json).expect("valid sample intent")
    }

    #[test]
    fn tc_3_1_0_05_is_usable_false_when_expired_despite_created_status() {
        let now = Utc::now();
        let expired_one_min_ago = sample_intent("created", now - Duration::minutes(1));
        assert!(!expired_one_min_ago.is_usable_at(now));
    }

    #[test]
    fn is_usable_respects_exact_3600s_boundary_and_non_created_statuses() {
        let now = Utc::now();
        let at_boundary = sample_intent("created", now);
        assert!(!at_boundary.is_usable_at(now));

        let still_valid = sample_intent("created", now + Duration::seconds(1));
        assert!(still_valid.is_usable_at(now));

        let fulfilled = sample_intent("fulfilled", now + Duration::minutes(30));
        assert!(!fulfilled.is_usable_at(now));

        let canceled = sample_intent("canceled", now + Duration::minutes(30));
        assert!(!canceled.is_usable_at(now));
    }
}
