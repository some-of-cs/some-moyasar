//! Typed error pipeline for `some-moyasar`.

use serde::Deserialize;

use crate::kernel::response_code::{FieldError, ResponseCode, deserialize_field_errors};

/// Top-level error classification returned by the Moyasar API.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiErrorType {
    /// Malformed or invalid request parameters (HTTP 400 / 422).
    #[serde(alias = "invalid_request")]
    InvalidRequestError,
    /// Invalid or unauthorized API key (HTTP 401).
    AuthenticationError,
    /// Requested resource was not found (HTTP 404).
    RecordNotFound,
    /// Payment intent cannot be canceled in its current state (HTTP 400 / 422).
    IntentNotCancelable,
    /// Internal Moyasar API failure (HTTP 5xx).
    ApiError,
    /// Forward-compatible fallback for unrecognized error types.
    #[serde(other)]
    Unknown,
}

fn default_api_error_type() -> ApiErrorType {
    ApiErrorType::Unknown
}

/// Raw JSON error body returned by the Moyasar API on non-2xx responses.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ApiErrorPayload {
    /// Top-level error classification (`type` in JSON).
    #[serde(rename = "type", default = "default_api_error_type")]
    pub error_type: ApiErrorType,
    /// Human-readable server message (`message` in JSON).
    #[serde(default)]
    pub message: Option<String>,
    /// Field-level validation errors (`errors` in JSON).
    #[serde(default, deserialize_with = "deserialize_field_errors")]
    pub errors: Vec<FieldError>,
    /// Optional two-digit ISO 8583 acquirer response code (`response_code` in JSON).
    #[serde(default)]
    pub response_code: Option<ResponseCode>,
}

impl From<ApiErrorPayload> for MoyasarError {
    fn from(payload: ApiErrorPayload) -> Self {
        Self::Api {
            error_type: payload.error_type,
            message: payload.message.unwrap_or_default(),
            errors: payload.errors,
            response_code: payload.response_code,
        }
    }
}

/// Exhaustive, compiler-checked error enum for all Moyasar SDK operations.
///
/// # Examples
///
/// ```
/// use some_moyasar::prelude::{ApiErrorType, MoyasarError};
///
/// let err = MoyasarError::from_api_response(
///     401,
///     r#"{"type":"authentication_error","message":"Invalid key"}"#,
/// );
/// assert!(matches!(
///     err,
///     MoyasarError::Api {
///         error_type: ApiErrorType::AuthenticationError,
///         ..
///     }
/// ));
/// assert!(!err.is_retryable());
/// ```
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
        matches!(
            self,
            Self::Network(_) | Self::Timeout | Self::ServerError { .. }
        )
    }

    /// Parses an HTTP status code and response body into a structured [`MoyasarError`].
    ///
    /// Valid Moyasar error JSON (`type`, `message`, `errors`, `response_code`) maps to
    /// [`MoyasarError::Api`]; unparseable 5xx bodies fall back to [`MoyasarError::ServerError`].
    pub fn from_api_response(status: u16, body: &str) -> Self {
        if let Ok(payload) = serde_json::from_str::<ApiErrorPayload>(body) {
            if status >= 500 {
                return Self::ServerError {
                    status,
                    message: payload.message.unwrap_or_default(),
                };
            }
            return Self::from(payload);
        }
        if status >= 500 {
            return Self::ServerError {
                status,
                message: body.to_owned(),
            };
        }
        Self::Api {
            error_type: ApiErrorType::Unknown,
            message: body.to_owned(),
            errors: Vec::new(),
            response_code: None,
        }
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

    #[test]
    fn tc_1_3_0_01_maps_401_authentication_error_payload() {
        let raw = r#"{"type":"authentication_error","message":"Invalid key"}"#;
        let err = MoyasarError::from_api_response(401, raw);
        match err {
            MoyasarError::Api {
                error_type,
                message,
                errors,
                response_code,
            } => {
                assert_eq!(error_type, ApiErrorType::AuthenticationError);
                assert_eq!(message, "Invalid key");
                assert!(errors.is_empty());
                assert_eq!(response_code, None);
            }
            other => panic!("expected MoyasarError::Api, got {other:?}"),
        }
    }

    #[test]
    fn tc_1_3_0_02_preserves_422_field_errors_array_and_map() {
        let array_json = r#"{
            "type": "invalid_request_error",
            "message": "Validation failed",
            "errors": [{ "field": "amount", "message": "must be positive" }],
            "response_code": "05"
        }"#;
        let err_array = MoyasarError::from_api_response(422, array_json);
        let expected_field = FieldError {
            field: "amount".into(),
            message: "must be positive".into(),
        };
        match err_array {
            MoyasarError::Api {
                error_type,
                errors,
                response_code,
                ..
            } => {
                assert_eq!(error_type, ApiErrorType::InvalidRequestError);
                assert_eq!(errors, vec![expected_field.clone()]);
                assert_eq!(response_code, Some(ResponseCode::DoNotHonor));
            }
            other => panic!("expected MoyasarError::Api, got {other:?}"),
        }

        let map_json = r#"{
            "type": "invalid_request",
            "message": "Validation failed",
            "errors": { "amount": ["must be positive"] }
        }"#;
        let err_map = MoyasarError::from_api_response(422, map_json);
        match err_map {
            MoyasarError::Api {
                error_type, errors, ..
            } => {
                assert_eq!(error_type, ApiErrorType::InvalidRequestError);
                assert_eq!(errors, vec![expected_field]);
            }
            other => panic!("expected MoyasarError::Api, got {other:?}"),
        }

        let not_found = MoyasarError::from_api_response(
            404,
            r#"{"type":"record_not_found","message":"Payment not found"}"#,
        );
        assert!(matches!(
            not_found,
            MoyasarError::Api {
                error_type: ApiErrorType::RecordNotFound,
                ..
            }
        ));

        let uncancelable = MoyasarError::from_api_response(
            400,
            r#"{"type":"intent_not_cancelable","message":"Cannot cancel"}"#,
        );
        assert!(matches!(
            uncancelable,
            MoyasarError::Api {
                error_type: ApiErrorType::IntentNotCancelable,
                ..
            }
        ));

        let fallback_5xx = MoyasarError::from_api_response(502, "Bad Gateway");
        assert!(matches!(
            fallback_5xx,
            MoyasarError::ServerError { status: 502, .. }
        ));
    }

    #[test]
    fn tc_1_3_0_law_error_formatting_hygiene_redacts_sensitive_data() {
        let errors = [
            MoyasarError::InvalidKey,
            MoyasarError::InvalidCardNumber,
            MoyasarError::CardExpired,
            MoyasarError::InvalidCvc,
            MoyasarError::from_api_response(
                401,
                r#"{"type":"authentication_error","message":"Unauthorized request"}"#,
            ),
        ];

        for err in errors {
            let formatted = format!("{err} | {err:?}");
            assert!(!formatted.contains("sk_live_"));
            assert!(!formatted.contains("sk_test_"));
            assert!(!formatted.contains("4111111111111111"));
            assert!(!formatted.contains("cvc: 123"));
        }
    }
}
