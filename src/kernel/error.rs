//! Typed error pipeline for `some-moyasar`.

use serde::Deserialize;

use crate::kernel::response_code::{FieldError, ResponseCode};

/// Top-level error classification returned by the Moyasar API.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiErrorType {
    /// Malformed or invalid request parameters (HTTP 400 / 422).
    InvalidRequestError,
    /// Invalid or unauthorized API key (HTTP 401).
    AuthenticationError,
    /// Internal Moyasar API failure (HTTP 5xx).
    ApiError,
    /// Forward-compatible fallback for unrecognized error types.
    #[serde(other)]
    Unknown,
}

/// Exhaustive, compiler-checked error enum for all Moyasar SDK operations.
#[derive(Debug, thiserror::Error)]
pub enum MoyasarError {
    /// Structured error payload returned by the Moyasar API.
    #[error("API error ({error_type:?}): {message}")]
    Api {
        /// Top-level error classification.
        error_type: ApiErrorType,
        /// Human-readable server message.
        message: String,
        /// Field-level validation errors.
        errors: Vec<FieldError>,
        /// Optional two-digit ISO 8583 acquirer response code.
        response_code: Option<ResponseCode>,
    },
    /// Underlying HTTP transport error.
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    /// Request exceeded the configured timeout.
    #[error("request timed out")]
    Timeout,
    /// Server-side 5xx failure.
    #[error("server error ({status}): {message}")]
    ServerError {
        /// HTTP status code.
        status: u16,
        /// Server error message.
        message: String,
    },
    /// API key failed prefix or format validation.
    #[error("invalid API key")]
    InvalidKey,
    /// Key type is not permitted for the target endpoint.
    #[error("key type mismatch for endpoint")]
    KeyTypeMismatch,
    /// Base URL must use HTTPS.
    #[error("invalid base URL (HTTPS required)")]
    InvalidBaseUrl,
    /// Metadata exceeds the 30-key limit.
    #[error("metadata exceeds 30 keys")]
    MetadataTooManyKeys,
    /// Metadata key exceeds 40 characters.
    #[error("metadata key exceeds 40 characters")]
    MetadataKeyTooLong,
    /// Metadata value exceeds 500 characters.
    #[error("metadata value exceeds 500 characters")]
    MetadataValueTooLong,
    /// Card number failed Luhn validation.
    #[error("invalid card number (Luhn check failed)")]
    InvalidCardNumber,
    /// Card expiration date is in the past.
    #[error("card is expired")]
    CardExpired,
    /// Card security code length is invalid.
    #[error("invalid CVC length")]
    InvalidCvc,
    /// Invalid Saudi mobile number format.
    #[error("invalid Saudi mobile number")]
    InvalidMobile,
    /// SADAD bill expiry exceeds 29 days.
    #[error("SADAD bill expiry exceeds 29 days")]
    SadadExpiryTooFar,
    /// Split amount must be strictly positive.
    #[error("split amount must be positive")]
    InvalidSplitAmount,
    /// Sum of splits exceeds total payment amount.
    #[error("split total exceeds payment amount")]
    SplitTotalExceedsAmount,
    /// At most one split may specify `fee_source: true`.
    #[error("at most one split may specify fee_source: true")]
    MultipleFeeSources,
    /// Token ID must start with `token_`.
    #[error("invalid token id prefix (expected token_)")]
    InvalidTokenId,
    /// Card auth ID must start with `ca_`.
    #[error("invalid card auth id prefix (expected ca_)")]
    InvalidCardAuthId,
    /// Saudi IBAN must match `SA` + 22 digits.
    #[error("invalid Saudi IBAN (expected SA + 22 digits)")]
    InvalidIban,
    /// Payout amount or sequence number is invalid.
    #[error("invalid payout amount or sequence number")]
    InvalidPayoutAmount,
    /// Webhook endpoint URL must use HTTPS.
    #[error("webhook endpoint URL must use HTTPS")]
    InvalidWebhookUrl,
}

impl MoyasarError {
    /// Returns `true` only for transient network, timeout, or 5xx server failures.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Network(_) | Self::Timeout | Self::ServerError { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_retryable_errors_correctly() {
        assert!(MoyasarError::Timeout.is_retryable());
        assert!(
            MoyasarError::ServerError {
                status: 503,
                message: "unavailable".into(),
            }
            .is_retryable()
        );
        assert!(!MoyasarError::InvalidKey.is_retryable());
        assert!(
            !MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                message: "bad request".into(),
                errors: vec![],
                response_code: None,
            }
            .is_retryable()
        );
    }
}
