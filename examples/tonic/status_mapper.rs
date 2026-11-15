//! Maps [`MoyasarError`] and [`ApiErrorType`] values to [`tonic::Status`] without orphan-rule conflicts.

use some_moyasar::prelude::{ApiErrorType, MoyasarError};

/// Newtype wrapper enabling `From<GrpcError> for tonic::Status` without violating orphan rules (E0117).
#[derive(Debug)]
pub struct GrpcError(pub MoyasarError);

impl From<GrpcError> for tonic::Status {
    fn from(err: GrpcError) -> Self {
        to_tonic_status(&err.0)
    }
}

const API_ERROR_CODE_MAP: &[(ApiErrorType, tonic::Code)] = &[
    (ApiErrorType::AuthenticationError, tonic::Code::Unauthenticated),
    (ApiErrorType::RecordNotFound, tonic::Code::NotFound),
    (ApiErrorType::InvalidRequestError, tonic::Code::InvalidArgument),
    (ApiErrorType::IntentNotCancelable, tonic::Code::InvalidArgument),
    (ApiErrorType::ApiError, tonic::Code::Unavailable),
    (ApiErrorType::Unknown, tonic::Code::Unknown),
];

const SECRET_PREFIXES: &[&str] = &["sk_live_", "sk_test_", "pk_live_", "pk_test_", "Bearer "];

fn api_error_type_to_code(error_type: &ApiErrorType) -> tonic::Code {
    API_ERROR_CODE_MAP
        .iter()
        .find(|(kind, _)| kind == error_type)
        .map_or(tonic::Code::Unknown, |(_, code)| *code)
}

fn sanitize_message(message: &str) -> String {
    if SECRET_PREFIXES.iter().any(|prefix| message.contains(prefix)) {
        return "upstream authentication or validation error".to_owned();
    }
    message.to_owned()
}

fn map_non_validation_error(err: &MoyasarError) -> Option<(tonic::Code, String)> {
    match err {
        MoyasarError::Api {
            error_type,
            message,
            ..
        } => Some((api_error_type_to_code(error_type), sanitize_message(message))),
        MoyasarError::Timeout => Some((tonic::Code::DeadlineExceeded, err.to_string())),
        MoyasarError::ServerError { status, .. } => Some((
            tonic::Code::Unavailable,
            format!("upstream server error (HTTP {status})"),
        )),
        MoyasarError::Network(_) => {
            Some((tonic::Code::Unavailable, "upstream network error".to_owned()))
        }
        _ => None,
    }
}

fn map_auth_or_webhook_error(err: &MoyasarError) -> Option<(tonic::Code, String)> {
    match err {
        MoyasarError::InvalidKey | MoyasarError::KeyTypeMismatch => {
            Some((tonic::Code::Unauthenticated, err.to_string()))
        }
        MoyasarError::InvalidWebhookSignature | MoyasarError::InvalidWebhookSecret => {
            Some((tonic::Code::PermissionDenied, err.to_string()))
        }
        _ => None,
    }
}

/// Converts a [`MoyasarError`] reference into a sanitized [`tonic::Status`].
pub fn to_tonic_status(err: &MoyasarError) -> tonic::Status {
    if let Some((code, msg)) = map_non_validation_error(err) {
        return tonic::Status::new(code, msg);
    }
    if let Some((code, msg)) = map_auth_or_webhook_error(err) {
        return tonic::Status::new(code, msg);
    }
    tonic::Status::new(tonic::Code::InvalidArgument, err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_api_error_types_to_expected_grpc_codes() {
        let cases = [
            (ApiErrorType::AuthenticationError, tonic::Code::Unauthenticated),
            (ApiErrorType::RecordNotFound, tonic::Code::NotFound),
            (ApiErrorType::InvalidRequestError, tonic::Code::InvalidArgument),
            (ApiErrorType::IntentNotCancelable, tonic::Code::InvalidArgument),
            (ApiErrorType::ApiError, tonic::Code::Unavailable),
            (ApiErrorType::Unknown, tonic::Code::Unknown),
        ];
        for (error_type, expected) in cases {
            let err = MoyasarError::Api {
                error_type,
                message: "gateway message".into(),
                errors: Vec::new(),
                response_code: None,
            };
            assert_eq!(to_tonic_status(&err).code(), expected);
        }
    }

    #[test]
    fn maps_webhook_timeout_server_and_validation_variants() {
        assert_eq!(
            to_tonic_status(&MoyasarError::InvalidWebhookSignature).code(),
            tonic::Code::PermissionDenied
        );
        assert_eq!(
            to_tonic_status(&MoyasarError::InvalidWebhookSecret).code(),
            tonic::Code::PermissionDenied
        );
        assert_eq!(
            to_tonic_status(&MoyasarError::Timeout).code(),
            tonic::Code::DeadlineExceeded
        );
        assert_eq!(
            to_tonic_status(&MoyasarError::ServerError {
                status: 503,
                message: "sk_live_should_not_leak".into(),
            })
            .code(),
            tonic::Code::Unavailable
        );
        assert_eq!(
            to_tonic_status(&MoyasarError::InvalidKey).code(),
            tonic::Code::Unauthenticated
        );
        assert_eq!(
            to_tonic_status(&MoyasarError::InvalidCardNumber).code(),
            tonic::Code::InvalidArgument
        );
    }

    #[test]
    fn grpc_error_newtype_converts_and_redacts_secrets() {
        let status: tonic::Status = GrpcError(MoyasarError::Api {
            error_type: ApiErrorType::AuthenticationError,
            message: "bad key sk_live_secret123".into(),
            errors: Vec::new(),
            response_code: None,
        })
        .into();
        assert_eq!(status.code(), tonic::Code::Unauthenticated);
        assert!(!status.message().contains("sk_live_"));
    }
}
