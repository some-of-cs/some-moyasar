//! Re-exports and validation helpers for webhook creation parameters and callback URLs.

use crate::kernel::error::MoyasarError;
pub use crate::resources::webhooks_api::crud::{
    CreateWebhookParams, validate_https_url, validate_webhook_url,
};

/// Validates a webhook callback URL (`https://`, non-empty host, no credentials or fragment).
///
/// # Errors
///
/// Returns [`MoyasarError::InvalidWebhookUrl`] if `url` is not a valid `https://` URL.
///
/// # Examples
///
/// ```
/// use some_moyasar::kernel::error::MoyasarError;
/// use some_moyasar::resources::webhooks_api::create_update::validate_webhook_callback_url;
///
/// assert!(validate_webhook_callback_url("https://merchant.example.com/webhooks").is_ok());
/// assert!(matches!(
///     validate_webhook_callback_url("http://merchant.example.com/webhooks"),
///     Err(MoyasarError::InvalidWebhookUrl)
/// ));
/// ```
pub fn validate_webhook_callback_url(url: &str) -> Result<(), MoyasarError> {
    validate_https_url(url)
}
