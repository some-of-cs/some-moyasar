//! Saved card token payment source (US-2.3.0 Unit 4).

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

fn serialize_opt_secret<S: Serializer>(
    secret: &Option<SecretString>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match secret {
        Some(value) => serializer.serialize_some(value.expose_secret()),
        None => serializer.serialize_none(),
    }
}

fn is_safe_token_suffix(suffix: &str) -> bool {
    !suffix.is_empty()
        && !suffix.contains("..")
        && !suffix
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, '/' | '?' | '#'))
}

fn validate_token_id(token: &str) -> Result<(), MoyasarError> {
    match token.strip_prefix("token_") {
        Some(suffix) if is_safe_token_suffix(suffix) => Ok(()),
        _ => Err(MoyasarError::InvalidTokenId),
    }
}

/// Saved card token payment source (`type: "token"`).
///
/// Charges a previously tokenized card using a `SecretKey` without 3DS by default
/// unless `three_ds` is explicitly set.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::payments::source_token::TokenSource;
///
/// let source = TokenSource::new("token_01J9SAVED").expect("valid token id");
/// source.validate().expect("passes validation");
/// assert!(format!("{source:?}").contains("[REDACTED]"));
/// ```
#[derive(Clone, Serialize)]
pub struct TokenSource {
    /// Saved card token identifier (`token_...`), zeroized on drop.
    #[serde(serialize_with = "serialize_secret")]
    pub token: SecretString,
    /// Optional 3-D Secure challenge override (`"3ds"` on the wire).
    #[serde(rename = "3ds", skip_serializing_if = "Option::is_none")]
    pub three_ds: Option<bool>,
    /// When `true`, authorizes without immediate capture (manual capture).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manual: Option<bool>,
    /// Optional CVC re-verification for saved card charges, zeroized on drop.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_opt_secret"
    )]
    pub cvc: Option<SecretString>,
}

impl TokenSource {
    /// Constructs and validates a [`TokenSource`] requiring a safe non-empty `token_` suffix.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::InvalidTokenId`] if `token` does not start with `"token_"`
    /// followed by a non-empty suffix free of whitespace, `/`, `?`, `#`, or `..`.
    pub fn new(token: impl Into<String>) -> Result<Self, MoyasarError> {
        let token_str = token.into();
        validate_token_id(&token_str)?;
        Ok(Self {
            token: SecretString::from(token_str),
            three_ds: None,
            manual: None,
            cvc: None,
        })
    }

    /// Validates the token prefix/suffix and optional CVC format.
    pub fn validate(&self) -> Result<(), MoyasarError> {
        validate_token_id(self.token.expose_secret())?;
        if let Some(cvc) = &self.cvc {
            let raw = cvc.expose_secret();
            let valid_len = raw.len() == 3 || raw.len() == 4;
            if !valid_len || !raw.bytes().all(|b| b.is_ascii_digit()) {
                return Err(MoyasarError::InvalidCvc);
            }
        }
        Ok(())
    }
}

impl fmt::Debug for TokenSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cvc_display = self.cvc.as_ref().map(|_| "[REDACTED]");
        f.debug_struct("TokenSource")
            .field("token", &"[REDACTED]")
            .field("three_ds", &self.three_ds)
            .field("manual", &self.manual)
            .field("cvc", &cvc_display)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_2_3_0_06_token_omits_3ds_when_unset_and_validates_prefix() {
        for invalid in [
            "token_",
            "abc_123",
            "token_abc/def",
            "token_abc?x=1",
            "token_abc#frag",
            "token_..",
            "token_a..b",
            "token_abc 123",
        ] {
            assert!(
                matches!(TokenSource::new(invalid), Err(MoyasarError::InvalidTokenId)),
                "expected InvalidTokenId for {invalid}"
            );
        }

        let src = TokenSource::new("token_abc").expect("valid token");
        let json = serde_json::to_value(&src).expect("serializes");
        assert_eq!(json["token"], "token_abc");
        assert!(json.get("3ds").is_none());

        let dbg = format!("{src:?}");
        assert!(dbg.contains("[REDACTED]"));
        assert!(!dbg.contains("token_abc"));
    }
}
