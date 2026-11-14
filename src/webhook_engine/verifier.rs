//! Constant-time Double-HMAC webhook secret verifier (US-7.3.0 Unit 2).
//!
//! Uses domain-separated HMAC-SHA256 (`DOMAIN_SEPARATION_KEY`) to map both the
//! configured `shared_secret` and inbound `event.secret_token` to fixed 32-byte
//! digests before comparing them with `subtle::ConstantTimeEq`. This prevents
//! both byte-by-byte timing side-channels and secret length leakage.

use std::fmt;
use std::time::Duration;

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use zeroize::Zeroize;

use super::event_types::{WebhookError, WebhookEvent};

/// Domain-separation key for Double-HMAC-SHA256 webhook secret verification.
pub const DOMAIN_SEPARATION_KEY: &[u8] = b"some-moyasar-webhook-domain-v1";

/// Constant-time Double-HMAC-SHA256 verifier for Moyasar webhook `secret_token`s.
///
/// Pre-computes a domain-separated 32-byte HMAC-SHA256 digest of the merchant's
/// `shared_secret` on construction and zeroizes the digest on [`Drop`].
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use chrono::Utc;
/// use some_moyasar::webhook_engine::verifier::ConstantTimeSecretVerifier;
///
/// let verifier = ConstantTimeSecretVerifier::new(
///     "whsec_live_123",
///     Duration::from_secs(4 * 60 * 60),
/// );
/// let now = Utc::now();
/// let payload = format!(
///     r#"{{
///         "id": "550e8400-e29b-41d4-a716-446655440000",
///         "type": "payment_paid",
///         "created_at": "{}",
///         "secret_token": "whsec_live_123",
///         "account_name": "Merchant",
///         "live": true,
///         "data": {{}}
///     }}"#,
///     now.to_rfc3339(),
/// );
///
/// let event = verifier
///     .verify_and_construct_at(payload.as_bytes(), now)
///     .expect("valid webhook should verify");
/// assert!(event.live);
/// ```
#[derive(Clone, Zeroize)]
pub struct ConstantTimeSecretVerifier {
    expected_secret_digest: [u8; 32],
    #[zeroize(skip)]
    tolerance: Duration,
}

impl Drop for ConstantTimeSecretVerifier {
    fn drop(&mut self) {
        self.expected_secret_digest.zeroize();
    }
}

impl fmt::Debug for ConstantTimeSecretVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConstantTimeSecretVerifier")
            .field("expected_secret_digest", &"[REDACTED]")
            .field("tolerance", &self.tolerance)
            .finish()
    }
}

impl ConstantTimeSecretVerifier {
    /// Creates a new [`ConstantTimeSecretVerifier`] by pre-computing the
    /// domain-separated HMAC-SHA256 digest of `shared_secret`.
    #[must_use]
    pub fn new(shared_secret: &str, tolerance: Duration) -> Self {
        Self {
            expected_secret_digest: Self::compute_digest(shared_secret.as_bytes()),
            tolerance,
        }
    }

    /// Computes a domain-separated 32-byte HMAC-SHA256 digest of `input`
    /// using [`DOMAIN_SEPARATION_KEY`].
    #[inline]
    #[must_use]
    pub fn compute_digest(input: &[u8]) -> [u8; 32] {
        let mut mac = Hmac::<Sha256>::new_from_slice(DOMAIN_SEPARATION_KEY)
            .expect("HMAC-SHA256 accepts keys of any size");
        mac.update(input);
        mac.finalize().into_bytes().into()
    }

    /// Returns the configured replay-window timestamp tolerance.
    #[must_use]
    pub const fn tolerance(&self) -> Duration {
        self.tolerance
    }

    /// Deserializes `raw_body` into a [`WebhookEvent`], verifies `secret_token`
    /// in constant time via Double-HMAC-SHA256 + `subtle::ConstantTimeEq`, and
    /// validates the event timestamp against `now` and `self.tolerance`.
    ///
    /// # Errors
    ///
    /// Returns:
    /// - [`WebhookError::InvalidJson`] if `raw_body` is not valid webhook JSON.
    /// - [`WebhookError::SecretMismatch`] if the Double-HMAC digest does not match.
    /// - [`WebhookError::TimestampOutOfRange`] if `created_at` falls outside the allowed window.
    pub fn verify_and_construct_at(
        &self,
        raw_body: &[u8],
        now: DateTime<Utc>,
    ) -> Result<WebhookEvent, WebhookError> {
        let event: WebhookEvent = serde_json::from_slice(raw_body)?;
        let mut provided_digest = Self::compute_digest(event.secret_token.as_bytes());
        let matched = provided_digest
            .ct_eq(&self.expected_secret_digest)
            .unwrap_u8()
            == 1;
        provided_digest.zeroize();
        if !matched {
            return Err(WebhookError::SecretMismatch);
        }
        let max_delta =
            chrono::TimeDelta::from_std(self.tolerance).unwrap_or(chrono::TimeDelta::MAX);
        let delta = now.signed_duration_since(event.created_at);
        if delta > max_delta || delta < -max_delta {
            return Err(WebhookError::TimestampOutOfRange);
        }
        Ok(event)
    }

    /// Verifies `raw_body` against [`Utc::now()`] and constructs a [`WebhookEvent`].
    ///
    /// # Errors
    ///
    /// Propagates any [`WebhookError`] from [`Self::verify_and_construct_at`].
    pub fn verify_and_construct(&self, raw_body: &[u8]) -> Result<WebhookEvent, WebhookError> {
        self.verify_and_construct_at(raw_body, Utc::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_payload(secret_token: &str, created_at: DateTime<Utc>) -> Vec<u8> {
        format!(
            r#"{{
                "id": "550e8400-e29b-41d4-a716-446655440000",
                "type": "payment_paid",
                "created_at": "{}",
                "secret_token": "{}",
                "account_name": "Merchant",
                "live": true,
                "data": {{"amount": 1000}}
            }}"#,
            created_at.to_rfc3339(),
            secret_token,
        )
        .into_bytes()
    }

    #[test]
    fn tc_7_3_0_01_valid_secret_token_accepted() {
        let now = Utc::now();
        let verifier =
            ConstantTimeSecretVerifier::new("whsec_live_123", Duration::from_secs(4 * 3600));
        let body = sample_payload("whsec_live_123", now);

        let event = verifier
            .verify_and_construct_at(&body, now)
            .expect("matching secret within tolerance should succeed");
        assert!(event.live);
    }

    #[test]
    fn tc_7_3_0_02_mismatched_length_secrets_fail_with_secret_mismatch() {
        let now = Utc::now();
        let verifier = ConstantTimeSecretVerifier::new(
            "whsec_32_byte_shared_secret_1234",
            Duration::from_secs(4 * 3600),
        );

        for candidate in ["x", "", "whsec_different_32_byte_secret_1", &"a".repeat(64)] {
            let body = sample_payload(candidate, now);
            let err = verifier
                .verify_and_construct_at(&body, now)
                .expect_err("wrong secret of any length must fail");
            assert!(
                matches!(err, WebhookError::SecretMismatch),
                "expected SecretMismatch for candidate length {}, got {err:?}",
                candidate.len()
            );
        }
    }

    #[test]
    fn invalid_json_returns_invalid_json_error() {
        let now = Utc::now();
        let verifier =
            ConstantTimeSecretVerifier::new("whsec_live_123", Duration::from_secs(4 * 3600));
        let err = verifier
            .verify_and_construct_at(b"{not-json", now)
            .expect_err("malformed JSON should fail");
        assert!(matches!(err, WebhookError::InvalidJson(_)));
    }

    #[test]
    fn compute_digest_is_deterministic_and_domain_separated() {
        let d1 = ConstantTimeSecretVerifier::compute_digest(b"secret-a");
        let d2 = ConstantTimeSecretVerifier::compute_digest(b"secret-a");
        let d3 = ConstantTimeSecretVerifier::compute_digest(b"secret-b");
        assert_eq!(d1, d2);
        assert_ne!(d1, d3);
        assert_eq!(d1.len(), 32);
    }

    #[test]
    fn debug_redacts_digest_and_tolerance_is_accessible() {
        let tolerance = Duration::from_secs(14_400);
        let verifier = ConstantTimeSecretVerifier::new("whsec_live_123", tolerance);
        let dbg = format!("{verifier:?}");
        assert!(dbg.contains("[REDACTED]"));
        assert_eq!(verifier.tolerance(), tolerance);
    }

    #[test]
    fn expired_timestamp_returns_timestamp_out_of_range() {
        let now = Utc::now();
        let verifier =
            ConstantTimeSecretVerifier::new("whsec_live_123", Duration::from_secs(300));
        let stale = now - chrono::TimeDelta::seconds(301);
        let body = sample_payload("whsec_live_123", stale);
        let err = verifier
            .verify_and_construct_at(&body, now)
            .expect_err("stale webhook timestamp must fail");
        assert!(matches!(err, WebhookError::TimestampOutOfRange));
    }
}
