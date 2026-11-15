//! Custom Rocket responder mapping [`MoyasarError`] into redacted JSON responses.

use std::fmt;
use std::io::Cursor;

use rocket::http::{ContentType, Status};
use rocket::request::Request;
use rocket::response::{self, Responder, Response};
use serde::Serialize;
use some_moyasar::prelude::{ApiErrorType, FieldError, MoyasarError};

/// Rocket [`Responder`] wrapper around [`MoyasarError`] that emits status-coded,
/// redacted JSON error payloads.
pub struct MoyasarResponder(pub MoyasarError);

impl fmt::Debug for MoyasarResponder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MoyasarResponder")
            .field("error_type", &error_type_str(&self.0))
            .field("message", &error_message(&self.0))
            .field("errors", &field_errors(&self.0))
            .finish()
    }
}

impl From<MoyasarError> for MoyasarResponder {
    fn from(err: MoyasarError) -> Self {
        Self(err)
    }
}

impl<'r> Responder<'r, 'static> for MoyasarResponder {
    fn respond_to(self, _request: &'r Request<'_>) -> response::Result<'static> {
        let status = map_status(&self.0);
        let body = build_error_json(&self.0);
        Response::build()
            .status(status)
            .header(ContentType::JSON)
            .sized_body(body.len(), Cursor::new(body))
            .ok()
    }
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error_type: &'a str,
    message: String,
    errors: Vec<FieldErrorBody<'a>>,
}

#[derive(Debug, Serialize)]
struct FieldErrorBody<'a> {
    field: &'a str,
    message: String,
}

fn map_status(err: &MoyasarError) -> Status {
    match err {
        MoyasarError::Api { error_type, .. } => api_error_status(error_type),
        MoyasarError::ServerError { status, .. } => server_error_status(*status),
        MoyasarError::Timeout => Status::GatewayTimeout,
        MoyasarError::Network(_) => Status::BadGateway,
        _ => map_client_status(err),
    }
}

fn api_error_status(error_type: &ApiErrorType) -> Status {
    match error_type {
        ApiErrorType::AuthenticationError => Status::Unauthorized,
        ApiErrorType::RecordNotFound => Status::NotFound,
        ApiErrorType::IntentNotCancelable => Status::UnprocessableEntity,
        ApiErrorType::ApiError => Status::BadGateway,
        _ => Status::BadRequest,
    }
}

fn server_error_status(status: u16) -> Status {
    Status::from_code(status)
        .filter(|s| s.code >= 500)
        .unwrap_or(Status::BadGateway)
}

fn map_client_status(err: &MoyasarError) -> Status {
    match err {
        MoyasarError::InvalidKey
        | MoyasarError::KeyTypeMismatch
        | MoyasarError::InvalidWebhookSecret
        | MoyasarError::InvalidWebhookSignature => Status::Unauthorized,
        _ => Status::UnprocessableEntity,
    }
}

fn build_error_json(err: &MoyasarError) -> String {
    let payload = ErrorBody {
        error_type: error_type_str(err),
        message: error_message(err),
        errors: field_errors(err),
    };
    serde_json::to_string(&payload).unwrap_or_else(|_| {
        r#"{"error_type":"internal_error","message":"failed to serialize error","errors":[]}"#
            .to_owned()
    })
}

fn error_type_str(err: &MoyasarError) -> &'static str {
    match err {
        MoyasarError::Api { error_type, .. } => api_error_type_str(error_type),
        MoyasarError::ServerError { .. } => "server_error",
        MoyasarError::Timeout => "timeout_error",
        MoyasarError::Network(_) => "network_error",
        _ => client_error_type_str(err),
    }
}

fn api_error_type_str(error_type: &ApiErrorType) -> &'static str {
    match error_type {
        ApiErrorType::InvalidRequestError => "invalid_request_error",
        ApiErrorType::AuthenticationError => "authentication_error",
        ApiErrorType::RecordNotFound => "record_not_found",
        ApiErrorType::IntentNotCancelable => "intent_not_cancelable",
        ApiErrorType::ApiError => "api_error",
        ApiErrorType::Unknown => "unknown_error",
    }
}

fn client_error_type_str(err: &MoyasarError) -> &'static str {
    match err {
        MoyasarError::InvalidKey
        | MoyasarError::KeyTypeMismatch
        | MoyasarError::InvalidWebhookSecret
        | MoyasarError::InvalidWebhookSignature => "authentication_error",
        _ => "invalid_request_error",
    }
}

fn error_message(err: &MoyasarError) -> String {
    match err {
        MoyasarError::Api { message, .. } => redact_sensitive(message),
        MoyasarError::ServerError { message, .. } => redact_sensitive(message),
        MoyasarError::Network(_) => "upstream network error".to_owned(),
        other => redact_sensitive(&other.to_string()),
    }
}

fn field_errors(err: &MoyasarError) -> Vec<FieldErrorBody<'_>> {
    match err {
        MoyasarError::Api { errors, .. } => errors.iter().map(to_field_error_body).collect(),
        _ => Vec::new(),
    }
}

fn to_field_error_body(fe: &FieldError) -> FieldErrorBody<'_> {
    FieldErrorBody {
        field: &fe.field,
        message: redact_sensitive(&fe.message),
    }
}

fn redact_sensitive(input: &str) -> String {
    input
        .split_whitespace()
        .map(redact_token)
        .collect::<Vec<_>>()
        .join(" ")
}

fn redact_token(token: &str) -> &str {
    if is_secret_token(token) || is_iban_token(token) || is_pan_token(token) {
        "[REDACTED]"
    } else {
        token
    }
}

fn is_secret_token(s: &str) -> bool {
    let trimmed = s.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
    trimmed.starts_with("sk_live_")
        || trimmed.starts_with("sk_test_")
        || trimmed.starts_with("whsec_")
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

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    #[test]
    fn from_moyasar_error_wraps_inner_error() {
        let responder = MoyasarResponder::from(MoyasarError::InvalidKey);
        assert!(matches!(responder.0, MoyasarError::InvalidKey));
    }

    #[test]
    fn maps_api_and_client_errors_to_expected_http_statuses() {
        let auth_err = MoyasarError::Api {
            error_type: ApiErrorType::AuthenticationError,
            message: "Invalid key".into(),
            errors: vec![],
            response_code: None,
        };
        assert_eq!(map_status(&auth_err), Status::Unauthorized);

        let not_found = MoyasarError::Api {
            error_type: ApiErrorType::RecordNotFound,
            message: "Not found".into(),
            errors: vec![],
            response_code: None,
        };
        assert_eq!(map_status(&not_found), Status::NotFound);

        let server_err = MoyasarError::ServerError {
            status: 503,
            message: "Unavailable".into(),
        };
        assert_eq!(map_status(&server_err), Status::ServiceUnavailable);
        assert_eq!(map_status(&MoyasarError::Timeout), Status::GatewayTimeout);
        assert_eq!(map_status(&MoyasarError::InvalidIban), Status::UnprocessableEntity);
        assert_eq!(map_status(&MoyasarError::InvalidWebhookSignature), Status::Unauthorized);
    }

    #[test]
    fn builds_structured_json_body_and_redacts_secrets_iban_and_pan() {
        let err = MoyasarError::Api {
            error_type: ApiErrorType::InvalidRequestError,
            message: "rejected sk_live_abc123 for SA0380000000608010167519 and 4111-1111-1111-1111".into(),
            errors: vec![FieldError {
                field: "iban".into(),
                message: "invalid SA0380000000608010167519 with whsec_test_999".into(),
            }],
            response_code: None,
        };

        let raw = build_error_json(&err);
        assert!(!raw.contains("sk_live_"));
        assert!(!raw.contains("whsec_"));
        assert!(!raw.contains("SA0380000000608010167519"));
        assert!(!raw.contains("4111-1111-1111-1111"));

        let parsed: Value = serde_json::from_str(&raw).expect("valid error JSON");
        assert_eq!(parsed["error_type"], "invalid_request_error");
        assert_eq!(parsed["message"], "rejected [REDACTED] for [REDACTED] and [REDACTED]");
        assert_eq!(parsed["errors"][0]["field"], "iban");
        assert_eq!(parsed["errors"][0]["message"], "invalid [REDACTED] with [REDACTED]");
    }

    #[test]
    fn debug_formatting_redacts_secrets_iban_and_pan() {
        let err = MoyasarError::Api {
            error_type: ApiErrorType::InvalidRequestError,
            message: "leaked sk_live_abc123 and 4111-1111-1111-1111".into(),
            errors: vec![FieldError {
                field: "iban".into(),
                message: "SA0380000000608010167519 with whsec_test_999".into(),
            }],
            response_code: None,
        };

        let debug_out = format!("{:?}", MoyasarResponder(err));
        assert!(!debug_out.contains("sk_live_"));
        assert!(!debug_out.contains("whsec_"));
        assert!(!debug_out.contains("SA0380000000608010167519"));
        assert!(!debug_out.contains("4111-1111-1111-1111"));
        assert!(debug_out.contains("MoyasarResponder"));
        assert!(debug_out.contains("[REDACTED]"));
    }
}
