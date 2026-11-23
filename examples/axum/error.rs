//! Redacted Axum `IntoResponse` error mapper for `MoyasarError`.
#![forbid(unsafe_code)]

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use some_moyasar::prelude::{ApiErrorType, MoyasarError};

const REDACTED_MARKERS: &[&str] = &[
    "sk_live_", "sk_test_", "pk_live_", "pk_test_", "whsec_", "Bearer ",
];

/// Wrapper around [`MoyasarError`] that converts SDK errors into redacted JSON HTTP responses.
#[derive(Debug)]
pub struct AppError(pub MoyasarError);

impl From<MoyasarError> for AppError {
    fn from(err: MoyasarError) -> Self {
        Self(err)
    }
}

#[derive(Serialize)]
struct ErrorDetail<'a> {
    code: &'a str,
    message: String,
}

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    error: ErrorDetail<'a>,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = map_moyasar_error(&self.0);
        let payload = ErrorEnvelope {
            error: ErrorDetail { code, message },
        };
        (status, Json(payload)).into_response()
    }
}

fn is_iban_token(s: &str) -> bool {
    let trimmed = s.trim_matches(|c: char| !c.is_alphanumeric());
    trimmed
        .strip_prefix("SA")
        .is_some_and(|digits| digits.len() == 22 && digits.bytes().all(|b| b.is_ascii_digit()))
}

fn is_pan_token(s: &str) -> bool {
    let trimmed = s.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    let clean: String = trimmed.chars().filter(|&c| c != '-').collect();
    (13..=19).contains(&clean.len()) && clean.bytes().all(|b| b.is_ascii_digit())
}

fn contains_sensitive_data(trimmed: &str) -> bool {
    REDACTED_MARKERS.iter().any(|m| trimmed.contains(m))
        || trimmed
            .split_whitespace()
            .any(|tok| is_iban_token(tok) || is_pan_token(tok))
}

fn sanitize_message(message: &str) -> String {
    let trimmed = message.trim();
    if trimmed.is_empty() || contains_sensitive_data(trimmed) {
        return "Request rejected by payment gateway.".to_owned();
    }
    trimmed.to_owned()
}

fn map_api_error(error_type: &ApiErrorType, message: &str) -> (StatusCode, &'static str, String) {
    match error_type {
        ApiErrorType::AuthenticationError => (
            StatusCode::UNAUTHORIZED,
            "authentication_error",
            "Authentication with payment gateway failed.".to_owned(),
        ),
        ApiErrorType::RecordNotFound => (
            StatusCode::NOT_FOUND,
            "record_not_found",
            sanitize_message(message),
        ),
        ApiErrorType::InvalidRequestError | ApiErrorType::IntentNotCancelable => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_request",
            sanitize_message(message),
        ),
        ApiErrorType::ApiError | ApiErrorType::Unknown => (
            StatusCode::BAD_GATEWAY,
            "api_error",
            "Upstream payment gateway error.".to_owned(),
        ),
    }
}

fn map_moyasar_error(err: &MoyasarError) -> (StatusCode, &'static str, String) {
    match err {
        MoyasarError::Api {
            error_type,
            message,
            ..
        } => map_api_error(error_type, message),
        MoyasarError::Network(_) | MoyasarError::Timeout | MoyasarError::ServerError { .. } => (
            StatusCode::BAD_GATEWAY,
            "upstream_error",
            "Upstream payment gateway error.".to_owned(),
        ),
        MoyasarError::InvalidKey
        | MoyasarError::KeyTypeMismatch
        | MoyasarError::InvalidBaseUrl
        | MoyasarError::MetadataTooManyKeys
        | MoyasarError::MetadataKeyTooLong
        | MoyasarError::MetadataValueTooLong
        | MoyasarError::InvalidCardNumber
        | MoyasarError::CardExpired
        | MoyasarError::InvalidCvc
        | MoyasarError::InvalidMobile
        | MoyasarError::SadadExpiryTooFar
        | MoyasarError::InvalidSplitAmount
        | MoyasarError::SplitTotalExceedsAmount
        | MoyasarError::MultipleFeeSources
        | MoyasarError::InvalidTokenId
        | MoyasarError::InvalidCardAuthId
        | MoyasarError::InvalidIban
        | MoyasarError::InvalidPayoutAmount
        | MoyasarError::InvalidWebhookUrl => {
            (StatusCode::BAD_REQUEST, "validation_error", err.to_string())
        }
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Internal payment processing error.".to_owned(),
        ),
    }
}
