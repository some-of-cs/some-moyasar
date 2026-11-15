//! Actix-Web `ResponseError` adapter for `MoyasarError`.

use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use some_moyasar::prelude::{ApiErrorType, MoyasarError};

/// Newtype wrapper adapting [`MoyasarError`] to Actix-Web's [`ResponseError`].
///
/// Maps SDK and upstream API errors to sanitized HTTP status codes and JSON
/// `{"error": "...", "code": "..."}` payloads without leaking credentials,
/// secret keys, or cardholder PAN data.
#[derive(Debug)]
pub struct AppError(pub MoyasarError);

impl From<MoyasarError> for AppError {
    fn from(err: MoyasarError) -> Self {
        Self(err)
    }
}

impl core::fmt::Display for AppError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let (_, code, message) = classify_error(&self.0);
        write!(f, "{code}: {message}")
    }
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        classify_error(&self.0).0
    }

    fn error_response(&self) -> HttpResponse {
        let (status, code, message) = classify_error(&self.0);
        HttpResponse::build(status).json(serde_json::json!({
            "error": message,
            "code": code,
        }))
    }
}

fn classify_error(err: &MoyasarError) -> (StatusCode, &'static str, &'static str) {
    if let MoyasarError::Api { error_type, .. } = err {
        return classify_api_error(error_type);
    }
    if let Some(mapped) = classify_auth_or_transport(err) {
        return mapped;
    }
    classify_validation_error(err)
}

fn classify_api_error(error_type: &ApiErrorType) -> (StatusCode, &'static str, &'static str) {
    match error_type {
        ApiErrorType::AuthenticationError => (
            StatusCode::UNAUTHORIZED,
            "authentication_error",
            "Upstream authentication failed",
        ),
        ApiErrorType::RecordNotFound => (
            StatusCode::NOT_FOUND,
            "record_not_found",
            "Requested payment resource was not found",
        ),
        ApiErrorType::InvalidRequestError => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_request_error",
            "Request parameters failed validation",
        ),
        ApiErrorType::IntentNotCancelable => (
            StatusCode::BAD_REQUEST,
            "intent_not_cancelable",
            "Payment intent cannot be canceled in its current state",
        ),
        ApiErrorType::ApiError | ApiErrorType::Unknown => (
            StatusCode::BAD_GATEWAY,
            "upstream_api_error",
            "Upstream payment gateway error",
        ),
    }
}

fn classify_auth_or_transport(
    err: &MoyasarError,
) -> Option<(StatusCode, &'static str, &'static str)> {
    match err {
        MoyasarError::InvalidKey | MoyasarError::KeyTypeMismatch => Some((
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "Invalid or mismatched API credentials",
        )),
        MoyasarError::InvalidWebhookSecret | MoyasarError::InvalidWebhookSignature => Some((
            StatusCode::UNAUTHORIZED,
            "webhook_verification_failed",
            "Webhook authentication failed",
        )),
        MoyasarError::Timeout => Some((
            StatusCode::GATEWAY_TIMEOUT,
            "gateway_timeout",
            "Upstream payment gateway timed out",
        )),
        MoyasarError::Network(_) | MoyasarError::ServerError { .. } => Some((
            StatusCode::BAD_GATEWAY,
            "upstream_gateway_error",
            "Upstream payment gateway request failed",
        )),
        _ => None,
    }
}

fn classify_validation_error(err: &MoyasarError) -> (StatusCode, &'static str, &'static str) {
    match err {
        MoyasarError::InvalidCardNumber | MoyasarError::CardExpired | MoyasarError::InvalidCvc => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_card",
            "Card validation failed",
        ),
        MoyasarError::InvalidMobile
        | MoyasarError::InvalidIban
        | MoyasarError::InvalidPayoutAmount => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_payment_details",
            "Payment details failed validation",
        ),
        MoyasarError::SadadExpiryTooFar
        | MoyasarError::InvalidSplitAmount
        | MoyasarError::SplitTotalExceedsAmount
        | MoyasarError::MultipleFeeSources => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_payment_rules",
            "Payment rules failed validation",
        ),
        _ => (
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Request parameters failed validation",
        ),
    }
}
