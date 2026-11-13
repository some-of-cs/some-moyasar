//! Pre-fulfillment usability and `client_secret` format guards.

use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::resources::payment_intents::models::PaymentIntent;

/// Validates that a `client_secret` string is non-empty and contains no surrounding whitespace.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::payment_intents::fulfill_guard::validate_client_secret;
///
/// assert!(validate_client_secret("cs_test_valid_123").is_ok());
/// assert!(validate_client_secret("").is_err());
/// assert!(validate_client_secret("   ").is_err());
/// ```
pub fn validate_client_secret(secret: &str) -> Result<(), MoyasarError> {
    if secret.trim().is_empty() {
        return Err(MoyasarError::Api {
            error_type: ApiErrorType::InvalidRequestError,
            message: "client_secret must not be empty".to_owned(),
            errors: Vec::new(),
            response_code: None,
        });
    }
    Ok(())
}

/// Verifies that `intent` is still usable (`is_usable()`) and `client_secret` is valid
/// before dispatching a browser fulfillment request.
pub fn validate_pre_fulfillment(
    intent: &PaymentIntent,
    client_secret: &str,
) -> Result<(), MoyasarError> {
    validate_client_secret(client_secret)?;
    if !intent.is_usable() {
        return Err(MoyasarError::Api {
            error_type: ApiErrorType::InvalidRequestError,
            message: "payment intent is expired or no longer usable".to_owned(),
            errors: Vec::new(),
            response_code: None,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn intent_with_expiry(status: &str, offset_secs: i64) -> PaymentIntent {
        let now = Utc::now();
        let expires_at = now + Duration::seconds(offset_secs);
        let created_at = expires_at - Duration::seconds(3600);
        let raw = format!(
            r#"{{
                "id": "pi_guard_1",
                "status": "{status}",
                "amount": 25000,
                "currency": "SAR",
                "client_secret": "cs_valid_secret",
                "expires_at": "{}",
                "created_at": "{}"
            }}"#,
            expires_at.to_rfc3339(),
            created_at.to_rfc3339()
        );
        serde_json::from_str(&raw).expect("valid intent")
    }

    #[test]
    fn rejects_empty_secret_and_expired_or_fulfilled_intents() {
        let active = intent_with_expiry("created", 1800);
        assert!(validate_pre_fulfillment(&active, "cs_valid_secret").is_ok());
        assert!(validate_pre_fulfillment(&active, "").is_err());

        let expired = intent_with_expiry("created", -60);
        assert!(validate_pre_fulfillment(&expired, "cs_valid_secret").is_err());

        let fulfilled = intent_with_expiry("fulfilled", 1800);
        assert!(validate_pre_fulfillment(&fulfilled, "cs_valid_secret").is_err());
    }
}
