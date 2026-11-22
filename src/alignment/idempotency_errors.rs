//! UUID v4 `given_id` validation and `3ds_auth_error` classification (US-9.1.0 Unit 4).
//!
//! Enforces two `some-payment` orchestrator alignment invariants for Moyasar:
//!
//! - **AC-4 / TM-5 ([`validate_given_id`])**: Rejects empty, malformed, and non-v4 UUIDs before
//!   forwarding `given_id` to Moyasar. Callers **must** rotate a declined `given_id` to a fresh
//!   UUID v4 on subsequent payment attempts.
//! - **AC-4 / TM-3 ([`map_moyasar_error`])**: Maps Moyasar `3ds_auth_error` responses strictly to
//!   [`MoyasarErrorClass::ThreeDsFailed`] and **never** to
//!   [`MoyasarErrorClass::AuthenticationError`], even when paired with HTTP `401` or
//!   `authentication_error`.

use uuid::{Uuid, Version};

use crate::kernel::error::MoyasarError;

/// Static lookup table mapping normalized Moyasar error types and codes to semantic classes.
const ERROR_CODE_TABLE: &[(&str, MoyasarErrorClass)] = &[
    ("3ds_auth_error", MoyasarErrorClass::ThreeDsFailed),
    ("authentication_error", MoyasarErrorClass::AuthenticationError),
    ("invalid_request_error", MoyasarErrorClass::ValidationError),
    ("validation_error", MoyasarErrorClass::ValidationError),
    ("card_declined", MoyasarErrorClass::CardDeclined),
    ("insufficient_funds", MoyasarErrorClass::CardDeclined),
    ("declined", MoyasarErrorClass::CardDeclined),
    ("rate_limit_error", MoyasarErrorClass::RateLimited),
];

/// Case-insensitive secret and token markers that must never appear in stored `vendor_code` values.
const REDACTED_MARKERS: &[&str] = &[
    "sk_live_", "sk_test_", "pk_live_", "pk_test_", "bearer ", "basic ",
];

/// Maximum length allowed for an unredacted vendor error code string.
const MAX_VENDOR_CODE_LEN: usize = 64;

/// High-level semantic classification of a Moyasar API or transport failure.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::idempotency_errors::{MoyasarErrorClass, map_moyasar_error};
///
/// let classified = map_moyasar_error(Some(401), Some("3ds_auth_error"), None);
/// assert_eq!(classified.class, MoyasarErrorClass::ThreeDsFailed);
/// assert_ne!(classified.class, MoyasarErrorClass::AuthenticationError);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MoyasarErrorClass {
    /// Request parameters failed schema or business validation (e.g., HTTP `400` / `422`).
    ValidationError,
    /// API key authentication or authorization failed (e.g., HTTP `401` / `403`).
    AuthenticationError,
    /// Cardholder 3-D Secure authentication failed (`3ds_auth_error`); never an API key failure.
    ThreeDsFailed,
    /// Issuer declined the card or reported insufficient funds.
    CardDeclined,
    /// Request was throttled by Moyasar rate limiting (HTTP `429` or `rate_limit_error`).
    RateLimited,
    /// Transport timeout, network failure, or HTTP `408` / `5xx` with indeterminate outcome.
    ProviderUnavailable,
    /// Unclassified provider-side error.
    ProviderError,
}

/// Structured, secret-safe classification of a Moyasar API or transport error.
///
/// Any `vendor_code` value containing credential markers (such as `sk_live_`, `sk_test_`,
/// `pk_live_`, `pk_test_`, `Bearer `, or `Basic `) or exceeding 64 bytes is automatically
/// replaced with `"[REDACTED]"` so [`core::fmt::Debug`] output never leaks secrets or tokens.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::idempotency_errors::{
///     ClassifiedMoyasarError, MoyasarErrorClass, map_moyasar_error,
/// };
///
/// let classified: ClassifiedMoyasarError = map_moyasar_error(Some(503), None, None);
/// assert_eq!(classified.class, MoyasarErrorClass::ProviderUnavailable);
/// assert_eq!(classified.http_status, Some(503));
/// assert!(classified.indeterminate);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassifiedMoyasarError {
    /// Semantic error class derived from vendor codes and HTTP status.
    pub class: MoyasarErrorClass,
    /// HTTP status code returned by Moyasar, or `None` for transport/timeout errors.
    pub http_status: Option<u16>,
    /// Sanitized vendor error code or type, redacted if it resembles a secret or token.
    pub vendor_code: Option<String>,
    /// `true` when the payment outcome is indeterminate (HTTP `408`, `5xx`, or transport failure).
    pub indeterminate: bool,
}

/// Validates that `raw` is a canonical UUID version 4 (`Random`) suitable for Moyasar `given_id`.
///
/// # Idempotency Rotation Rule (TM-5)
///
/// Moyasar binds each `given_id` to a single payment attempt. If a payment attempt is declined,
/// callers **must** rotate the declined `given_id` to a newly generated UUID v4 before submitting
/// a subsequent attempt. Reusing a declined `given_id` causes Moyasar to replay the declined
/// outcome instead of executing a fresh charge.
///
/// # Errors
///
/// Returns [`MoyasarError::Api`] with `InvalidRequestError` when `raw` (after trimming
/// surrounding whitespace) is empty, not a valid UUID string, or any UUID version other than v4
/// (such as v1, v5, or the nil UUID). Raw input is intentionally omitted from error messages to
/// prevent accidental credential leakage.
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::idempotency_errors::validate_given_id;
/// use uuid::Uuid;
///
/// let fresh = Uuid::new_v4().to_string();
/// let parsed = validate_given_id(&fresh).expect("valid UUID v4 must be accepted");
/// assert_eq!(parsed.to_string(), fresh);
///
/// assert!(validate_given_id("00000000-0000-0000-0000-000000000000").is_err());
/// ```
pub fn validate_given_id(raw: &str) -> Result<Uuid, MoyasarError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(MoyasarError::invalid_request(
            "given_id must not be empty; provide a fresh UUID v4",
        ));
    }

    let parsed = Uuid::parse_str(trimmed).map_err(|_| {
        MoyasarError::invalid_request("given_id must be a valid canonical UUID v4 string")
    })?;

    if parsed.get_version() != Some(Version::Random) {
        return Err(MoyasarError::invalid_request(
            "given_id must be a UUID version 4 (random); non-v4 UUIDs are rejected",
        ));
    }

    Ok(parsed)
}

/// Classifies a Moyasar API or transport failure into a [`ClassifiedMoyasarError`].
///
/// Precedence rules:
/// 1. `"3ds_auth_error"` in either `error_type` or `error_code` (case-insensitive) always maps
///    to [`MoyasarErrorClass::ThreeDsFailed`] and **never** to
///    [`MoyasarErrorClass::AuthenticationError`] (AC-4, TM-3).
/// 2. Known vendor codes/types are resolved via a static lookup table (`error_code` before
///    `error_type`).
/// 3. Remaining responses fall back to `http_status` (`429` -> [`MoyasarErrorClass::RateLimited`],
///    `408` / `500..=599` / `None` -> [`MoyasarErrorClass::ProviderUnavailable`] with
///    `indeterminate: true`, `401` / `403` -> [`MoyasarErrorClass::AuthenticationError`],
///    `400` / `422` -> [`MoyasarErrorClass::ValidationError`], otherwise
///    [`MoyasarErrorClass::ProviderError`]).
///
/// # Examples
///
/// ```
/// use some_moyasar::alignment::idempotency_errors::{MoyasarErrorClass, map_moyasar_error};
///
/// // 3DS auth errors never misclassify as API key authentication errors:
/// let tds = map_moyasar_error(Some(401), Some("authentication_error"), Some("3ds_auth_error"));
/// assert_eq!(tds.class, MoyasarErrorClass::ThreeDsFailed);
/// assert!(!tds.indeterminate);
///
/// // Transport timeouts are marked indeterminate:
/// let timeout = map_moyasar_error(None, None, None);
/// assert_eq!(timeout.class, MoyasarErrorClass::ProviderUnavailable);
/// assert!(timeout.indeterminate);
/// ```
#[must_use]
pub fn map_moyasar_error(
    http_status: Option<u16>,
    error_type: Option<&str>,
    error_code: Option<&str>,
) -> ClassifiedMoyasarError {
    let class = if is_three_ds_auth_error(error_code) || is_three_ds_auth_error(error_type) {
        MoyasarErrorClass::ThreeDsFailed
    } else {
        classify_by_code(error_code)
            .or_else(|| classify_by_code(error_type))
            .unwrap_or_else(|| classify_by_status(http_status))
    };

    ClassifiedMoyasarError {
        class,
        http_status,
        vendor_code: resolve_vendor_code(error_code, error_type),
        indeterminate: class == MoyasarErrorClass::ProviderUnavailable,
    }
}

fn is_three_ds_auth_error(value: Option<&str>) -> bool {
    value.is_some_and(|raw| raw.trim().eq_ignore_ascii_case("3ds_auth_error"))
}

fn classify_by_code(value: Option<&str>) -> Option<MoyasarErrorClass> {
    let normalized = value?.trim();
    ERROR_CODE_TABLE
        .iter()
        .find(|(code, _)| normalized.eq_ignore_ascii_case(code))
        .map(|(_, class)| *class)
}

fn classify_by_status(http_status: Option<u16>) -> MoyasarErrorClass {
    match http_status {
        None | Some(408 | 500..=599) => MoyasarErrorClass::ProviderUnavailable,
        Some(429) => MoyasarErrorClass::RateLimited,
        Some(401 | 403) => MoyasarErrorClass::AuthenticationError,
        Some(400 | 422) => MoyasarErrorClass::ValidationError,
        Some(_) => MoyasarErrorClass::ProviderError,
    }
}

fn contains_sensitive_marker(lower: &str) -> bool {
    REDACTED_MARKERS.iter().any(|marker| lower.contains(marker))
}

fn sanitize_vendor_code(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if trimmed.len() > MAX_VENDOR_CODE_LEN || contains_sensitive_marker(&lower) {
        return Some("[REDACTED]".to_owned());
    }
    Some(trimmed.to_owned())
}

fn resolve_vendor_code(error_code: Option<&str>, error_type: Option<&str>) -> Option<String> {
    error_code
        .and_then(sanitize_vendor_code)
        .or_else(|| error_type.and_then(sanitize_vendor_code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_9_1_4_accepts_valid_uuid_v4_with_optional_whitespace() {
        let generated = Uuid::new_v4();
        let raw = format!("  {generated}  ");
        let parsed = validate_given_id(&raw).expect("valid UUID v4 must succeed");
        assert_eq!(parsed, generated);
        assert_eq!(parsed.get_version(), Some(Version::Random));
    }

    #[test]
    fn tc_9_1_4_rejects_empty_garbage_nil_v1_and_v5_uuids() {
        let v5 = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"https://moyasar.com/payment/1").to_string();
        let v1 = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
        let nil = Uuid::nil().to_string();

        for invalid in ["", "   ", "not-a-uuid", &nil, v1, &v5] {
            assert!(
                validate_given_id(invalid).is_err(),
                "expected rejection for given_id input: {invalid}"
            );
        }
    }

    #[test]
    fn tc_9_1_4_three_ds_auth_error_never_maps_to_authentication_error() {
        let cases = [
            (Some(401), Some("3ds_auth_error"), None),
            (Some(400), Some("3ds_auth_error"), None),
            (Some(401), Some("authentication_error"), Some("3ds_auth_error")),
            (Some(400), Some("invalid_request_error"), Some("3DS_AUTH_ERROR")),
            (Some(403), None, Some("  3ds_auth_error  ")),
        ];

        for (status, err_type, err_code) in cases {
            let classified = map_moyasar_error(status, err_type, err_code);
            assert_eq!(classified.class, MoyasarErrorClass::ThreeDsFailed);
            assert_ne!(classified.class, MoyasarErrorClass::AuthenticationError);
            assert!(!classified.indeterminate);
            assert_eq!(classified.http_status, status);
        }
    }

    #[test]
    fn tc_9_1_4_maps_authentication_validation_and_card_declined_errors() {
        let auth = map_moyasar_error(Some(401), Some("authentication_error"), None);
        assert_eq!(auth.class, MoyasarErrorClass::AuthenticationError);
        assert!(!auth.indeterminate);

        let auth_403 = map_moyasar_error(Some(403), None, None);
        assert_eq!(auth_403.class, MoyasarErrorClass::AuthenticationError);

        for (status, err_type) in [
            (Some(400), Some("invalid_request_error")),
            (Some(422), Some("validation_error")),
            (Some(400), None),
            (Some(422), None),
        ] {
            let classified = map_moyasar_error(status, err_type, None);
            assert_eq!(classified.class, MoyasarErrorClass::ValidationError);
            assert!(!classified.indeterminate);
        }

        for code in ["card_declined", "insufficient_funds", "declined"] {
            let classified = map_moyasar_error(Some(402), None, Some(code));
            assert_eq!(classified.class, MoyasarErrorClass::CardDeclined);
            assert!(!classified.indeterminate);
            assert_eq!(classified.vendor_code.as_deref(), Some(code));
        }
    }

    #[test]
    fn tc_9_1_4_maps_rate_limited_unavailable_and_fallback_errors() {
        let rate_429 = map_moyasar_error(Some(429), None, None);
        assert_eq!(rate_429.class, MoyasarErrorClass::RateLimited);
        assert!(!rate_429.indeterminate);

        let rate_type = map_moyasar_error(Some(400), Some("rate_limit_error"), None);
        assert_eq!(rate_type.class, MoyasarErrorClass::RateLimited);
        assert!(!rate_type.indeterminate);

        for status in [Some(408), Some(500), Some(502), Some(503), None] {
            let unavailable = map_moyasar_error(status, None, None);
            assert_eq!(unavailable.class, MoyasarErrorClass::ProviderUnavailable);
            assert!(unavailable.indeterminate);
            assert_eq!(unavailable.http_status, status);
        }

        let fallback = map_moyasar_error(Some(418), Some("unexpected_vendor_code"), None);
        assert_eq!(fallback.class, MoyasarErrorClass::ProviderError);
        assert!(!fallback.indeterminate);
    }

    #[test]
    fn redacts_secrets_and_tokens_from_vendor_code_and_debug_output() {
        let secret_inputs = [
            "sk_live_0123456789abcdef",
            "sk_test_abcdef0123456789",
            "Bearer eyJhbGciOiJIUzI1NiJ9.payload",
            "Basic c2tfbGl2ZV9zZWNyZXQ6",
        ];

        for secret in secret_inputs {
            let classified = map_moyasar_error(Some(401), Some(secret), None);
            assert_eq!(classified.vendor_code.as_deref(), Some("[REDACTED]"));
            let debug_repr = format!("{classified:?}");
            assert!(!debug_repr.contains(secret));
            assert!(!debug_repr.contains("sk_live_"));
            assert!(!debug_repr.contains("sk_test_"));
            assert!(!debug_repr.contains("Bearer "));
        }
    }
}
