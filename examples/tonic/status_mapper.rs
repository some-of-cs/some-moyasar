//! Maps [`MoyasarError`] and [`ApiErrorType`] values to [`tonic::Status`] without orphan-rule conflicts.

#![allow(dead_code)]

use some_moyasar::prelude::{ApiErrorType, MoyasarError};

/// Newtype wrapper enabling `From<GrpcError> for tonic::Status` without violating orphan rules (E0117).
#[derive(Debug)]
pub struct GrpcError(pub MoyasarError);

impl From<GrpcError> for tonic::Status {
    fn from(err: GrpcError) -> Self {
        to_tonic_status(&err.0)
    }
}

/// Static lookup table mapping [`ApiErrorType`] variants to canonical gRPC status codes and safe messages.
pub const API_ERROR_MAP: &[(ApiErrorType, tonic::Code, &str)] = &[
    (
        ApiErrorType::AuthenticationError,
        tonic::Code::Unauthenticated,
        "upstream authentication error",
    ),
    (
        ApiErrorType::RecordNotFound,
        tonic::Code::NotFound,
        "requested resource not found",
    ),
    (
        ApiErrorType::InvalidRequestError,
        tonic::Code::InvalidArgument,
        "invalid request parameters",
    ),
    (
        ApiErrorType::IntentNotCancelable,
        tonic::Code::InvalidArgument,
        "payment intent is not cancelable",
    ),
    (
        ApiErrorType::ApiError,
        tonic::Code::Unavailable,
        "upstream gateway error",
    ),
    (
        ApiErrorType::Unknown,
        tonic::Code::Unknown,
        "unexpected gateway error",
    ),
];

fn map_api_error_type(error_type: &ApiErrorType) -> (tonic::Code, String) {
    API_ERROR_MAP
        .iter()
        .find(|(kind, _, _)| kind == error_type)
        .map_or_else(
            || (tonic::Code::Unknown, "unexpected gateway error".to_owned()),
            |(_, code, message)| (*code, (*message).to_owned()),
        )
}

fn map_non_validation_error(err: &MoyasarError) -> Option<(tonic::Code, String)> {
    match err {
        MoyasarError::Api { error_type, .. } => Some(map_api_error_type(error_type)),
        MoyasarError::Timeout => Some((tonic::Code::DeadlineExceeded, err.to_string())),
        MoyasarError::ServerError { status, .. } => Some((
            tonic::Code::Unavailable,
            format!("upstream server error (HTTP {status})"),
        )),
        MoyasarError::Network(_) => Some((
            tonic::Code::Unavailable,
            "upstream network error".to_owned(),
        )),
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
    fn maps_api_error_types_to_expected_grpc_codes_and_canonical_messages() {
        for (error_type, expected_code, expected_msg) in API_ERROR_MAP {
            let err = MoyasarError::Api {
                error_type: error_type.clone(),
                message: "raw upstream gateway error detail".into(),
                errors: Vec::new(),
                response_code: None,
            };
            let status = to_tonic_status(&err);
            assert_eq!(status.code(), *expected_code);
            assert_eq!(status.message(), *expected_msg);
        }
    }

    #[test]
    fn raw_non_json_upstream_4xx_body_is_never_exposed_in_status_message() {
        let raw_upstream_body =
            "<html><body>403 Forbidden: nginx/1.24 internal upstream trace id=99182</body></html>";
        let err = MoyasarError::Api {
            error_type: ApiErrorType::InvalidRequestError,
            message: raw_upstream_body.to_owned(),
            errors: Vec::new(),
            response_code: None,
        };
        let status = to_tonic_status(&err);
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
        assert_eq!(status.message(), "invalid request parameters");
        assert!(!status.message().contains("nginx"));
        assert!(!status.message().contains("Forbidden"));
        assert!(!status.message().contains(raw_upstream_body));
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
        assert_eq!(status.message(), "upstream authentication error");
        assert!(!status.message().contains("sk_live_"));
    }
}
