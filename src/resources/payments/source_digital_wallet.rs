//! Apple Pay and Samsung Pay digital wallet payment sources (US-2.3.0 Unit 3).

use core::fmt;

use secrecy::{ExposeSecret, SecretString};
use serde::{Serialize, Serializer};

use crate::kernel::error::MoyasarError;

fn serialize_secret<S: Serializer>(
    secret: &SecretString,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(secret.expose_secret())
}

/// Apple Pay payment source (`type: "applepay"`).
///
/// The encrypted wallet `token` / cryptogram is stored in [`SecretString`] and
/// redacted as `"[REDACTED]"` in [`fmt::Debug`] output.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::payments::source_digital_wallet::ApplePaySource;
///
/// let apple = ApplePaySource::new("apple_pay_encrypted_token_payload");
/// apple.validate().expect("non-empty token");
/// assert!(format!("{apple:?}").contains("[REDACTED]"));
/// ```
#[derive(Clone, Serialize)]
pub struct ApplePaySource {
    /// Encrypted Apple Pay payment token payload.
    #[serde(serialize_with = "serialize_secret")]
    pub token: SecretString,
    /// When `true`, authorizes without immediate capture (manual capture).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manual: Option<bool>,
    /// When `true`, tokenizes the underlying card when supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save_card: Option<bool>,
    /// Optional statement descriptor shown on the cardholder's bank statement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statement_descriptor: Option<String>,
}

impl ApplePaySource {
    /// Creates a new [`ApplePaySource`] from the encrypted Apple Pay token string.
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: SecretString::from(token.into()),
            manual: None,
            save_card: None,
            statement_descriptor: None,
        }
    }

    /// Validates that the wallet token payload is non-empty.
    pub fn validate(&self) -> Result<(), MoyasarError> {
        if self.token.expose_secret().trim().is_empty() {
            return Err(MoyasarError::InvalidTokenId);
        }
        Ok(())
    }
}

impl fmt::Debug for ApplePaySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApplePaySource")
            .field("token", &"[REDACTED]")
            .field("manual", &self.manual)
            .field("save_card", &self.save_card)
            .field("statement_descriptor", &self.statement_descriptor)
            .finish()
    }
}

/// Samsung Pay payment source (`type: "samsungpay"`).
///
/// The encrypted wallet `token` / cryptogram is stored in [`SecretString`] and
/// redacted as `"[REDACTED]"` in [`fmt::Debug`] output.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::payments::source_digital_wallet::SamsungPaySource;
///
/// let samsung = SamsungPaySource::new("samsung_pay_encrypted_token_payload");
/// samsung.validate().expect("non-empty token");
/// assert!(format!("{samsung:?}").contains("[REDACTED]"));
/// ```
#[derive(Clone, Serialize)]
pub struct SamsungPaySource {
    /// Encrypted Samsung Pay payment token payload.
    #[serde(serialize_with = "serialize_secret")]
    pub token: SecretString,
    /// When `true`, authorizes without immediate capture (manual capture).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manual: Option<bool>,
    /// When `true`, tokenizes the underlying card when supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save_card: Option<bool>,
    /// Optional statement descriptor shown on the cardholder's bank statement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statement_descriptor: Option<String>,
}

impl SamsungPaySource {
    /// Creates a new [`SamsungPaySource`] from the encrypted Samsung Pay token string.
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: SecretString::from(token.into()),
            manual: None,
            save_card: None,
            statement_descriptor: None,
        }
    }

    /// Validates that the wallet token payload is non-empty.
    pub fn validate(&self) -> Result<(), MoyasarError> {
        if self.token.expose_secret().trim().is_empty() {
            return Err(MoyasarError::InvalidTokenId);
        }
        Ok(())
    }
}

impl fmt::Debug for SamsungPaySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SamsungPaySource")
            .field("token", &"[REDACTED]")
            .field("manual", &self.manual)
            .field("save_card", &self.save_card)
            .field("statement_descriptor", &self.statement_descriptor)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallet_debug_redacts_token_and_cryptogram() {
        let apple = ApplePaySource::new("apple_cryptogram_secret_xyz");
        let samsung = SamsungPaySource::new("samsung_cryptogram_secret_xyz");
        let apple_dbg = format!("{apple:?}");
        let samsung_dbg = format!("{samsung:?}");
        assert!(apple_dbg.contains("[REDACTED]"));
        assert!(!apple_dbg.contains("apple_cryptogram_secret_xyz"));
        assert!(samsung_dbg.contains("[REDACTED]"));
        assert!(!samsung_dbg.contains("samsung_cryptogram_secret_xyz"));
    }

    #[test]
    fn rejects_empty_wallet_token() {
        assert!(matches!(
            ApplePaySource::new("   ").validate(),
            Err(MoyasarError::InvalidTokenId)
        ));
        assert!(matches!(
            SamsungPaySource::new("").validate(),
            Err(MoyasarError::InvalidTokenId)
        ));
    }
}
