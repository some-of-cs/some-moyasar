//! Card token domain models (`Token`, `TokenStatus`, `TokenId`).
//!
//! Enforces PCI-DSS 4.0 Requirement 3.4 tokenization invariants: full PANs and
//! CVCs are never stored on [`Token`]; only the `token_...` identifier and
//! masked card details are retained.

use core::fmt;
use core::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, de};

use crate::kernel::error::MoyasarError;

/// Required prefix for all Moyasar card token identifiers.
pub const TOKEN_ID_PREFIX: &str = "token_";

/// Validated Moyasar card token identifier (`token_...`).
///
/// Guarantees at construction and deserialization time that the value starts
/// with [`TOKEN_ID_PREFIX`] (`"token_"`) and has a non-empty, URL-safe suffix.
///
/// # Examples
///
/// ```
/// use some_moyasar::prelude::MoyasarError;
/// use some_moyasar::resources::tokens::models::TokenId;
///
/// let id = TokenId::new("token_abc123").expect("valid token id");
/// assert_eq!(id.as_str(), "token_abc123");
///
/// assert!(matches!(TokenId::new("token_"), Err(MoyasarError::InvalidTokenId)));
/// assert!(matches!(TokenId::new("pay_123"), Err(MoyasarError::InvalidTokenId)));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct TokenId(String);

fn is_valid_id_char(c: char) -> bool {
    !c.is_whitespace() && !"/?#\\".contains(c)
}

fn is_valid_token_suffix(suffix: &str) -> bool {
    !suffix.is_empty() && !suffix.contains("..") && suffix.chars().all(is_valid_id_char)
}

impl TokenId {
    /// Constructs a validated [`TokenId`], rejecting missing `token_` prefixes
    /// or empty/unsafe suffixes.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::InvalidTokenId`] when `raw` does not start with
    /// `"token_"` or has an empty or path-unsafe suffix after the prefix.
    pub fn new(raw: impl Into<String>) -> Result<Self, MoyasarError> {
        let raw = raw.into();
        if raw
            .strip_prefix(TOKEN_ID_PREFIX)
            .is_some_and(is_valid_token_suffix)
        {
            return Ok(Self(raw));
        }
        Err(MoyasarError::InvalidTokenId)
    }

    /// Borrows the underlying token identifier string without cloning.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for TokenId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TokenId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for TokenId {
    type Err = MoyasarError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl<'de> Deserialize<'de> for TokenId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::new(raw).map_err(de::Error::custom)
    }
}

/// Lifecycle state of a Moyasar card token (`initiated -> active -> inactive`).
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::tokens::models::TokenStatus;
///
/// let active: TokenStatus = serde_json::from_str(r#""active""#).unwrap();
/// assert_eq!(active, TokenStatus::Active);
/// assert!(active.is_active());
/// assert_eq!(active.as_str(), "active");
///
/// let unknown: TokenStatus = serde_json::from_str(r#""future_state""#).unwrap();
/// assert_eq!(unknown, TokenStatus::Unknown);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TokenStatus {
    /// Token was created (for example with `save_only: true`) and awaits
    /// cardholder verification.
    Initiated,
    /// Cardholder verification succeeded; token is eligible for off-session
    /// charges.
    Active,
    /// Cardholder verification failed, the card expired, or the token was
    /// deactivated.
    Inactive,
    /// Forward-compatible fallback for unrecognized status values.
    #[serde(other)]
    Unknown,
}

const TOKEN_STATUS_LABELS: &[(TokenStatus, &str)] = &[
    (TokenStatus::Initiated, "initiated"),
    (TokenStatus::Active, "active"),
    (TokenStatus::Inactive, "inactive"),
    (TokenStatus::Unknown, "unknown"),
];

impl TokenStatus {
    /// Returns the lowercase wire representation of this token status via a
    /// static lookup slice.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        TOKEN_STATUS_LABELS
            .iter()
            .find(|(status, _)| *status == self)
            .map_or("unknown", |(_, label)| *label)
    }

    /// Returns `true` when the token is [`TokenStatus::Active`] and ready for
    /// off-session charges.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }
}

impl fmt::Display for TokenStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Tokenized card resource returned by Moyasar's `/v1/tokens` endpoints.
///
/// Contains only the opaque [`TokenId`], lifecycle [`TokenStatus`], and masked
/// card display attributes — never a raw PAN or CVC.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::tokens::models::{Token, TokenStatus};
///
/// let json = r#"{
///     "id": "token_123456",
///     "status": "active",
///     "number": "XXXX-XXXX-XXXX-1111",
///     "company": "visa",
///     "gateway_id": "gw_1",
///     "created_at": "2026-01-15T10:30:00Z"
/// }"#;
///
/// let token: Token = serde_json::from_str(json).unwrap();
/// assert!(token.is_active());
/// assert_eq!(token.status, TokenStatus::Active);
/// assert_eq!(token.last_four(), Some("1111"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    /// Validated `token_...` identifier.
    pub id: TokenId,
    /// Current lifecycle state of the token.
    pub status: TokenStatus,
    /// Masked card number for customer display (for example `XXXX-XXXX-XXXX-1111`).
    pub number: String,
    /// Card network / scheme name when provided by Moyasar (for example `visa`).
    #[serde(default)]
    pub company: Option<String>,
    /// Optional gateway identifier associated with the token.
    #[serde(default)]
    pub gateway_id: Option<String>,
    /// UTC timestamp when the token was created.
    pub created_at: DateTime<Utc>,
}

impl Token {
    /// Returns `true` if this token is currently [`TokenStatus::Active`].
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.status.is_active()
    }

    /// Returns the masked card number suitable for customer display.
    #[must_use]
    pub fn masked_number(&self) -> &str {
        &self.number
    }

    /// Extracts the trailing four ASCII digits from the masked card number, if
    /// present.
    #[must_use]
    pub fn last_four(&self) -> Option<&str> {
        let start = self.number.len().checked_sub(4)?;
        let tail = self.number.get(start..)?;
        tail.bytes().all(|b| b.is_ascii_digit()).then_some(tail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contains_16_consecutive_digits(input: &str) -> bool {
        let mut run = 0usize;
        for byte in input.bytes() {
            if byte.is_ascii_digit() {
                run += 1;
                if run >= 16 {
                    return true;
                }
            } else {
                run = 0;
            }
        }
        false
    }

    #[test]
    fn token_id_validates_prefix_and_non_empty_suffix() {
        let id = TokenId::new("token_abc123").expect("valid token id");
        assert_eq!(id.as_str(), "token_abc123");
        assert_eq!(id.as_ref(), "token_abc123");
        assert_eq!(id.to_string(), "token_abc123");

        let parsed: TokenId = "token_xyz".parse().expect("FromStr succeeds");
        assert_eq!(parsed.as_str(), "token_xyz");

        let roundtrip: TokenId = serde_json::from_str(r#""token_serde_1""#).unwrap();
        assert_eq!(roundtrip, TokenId::new("token_serde_1").unwrap());

        for invalid in [
            "pay_",
            "ca_",
            "",
            "token_",
            "tok_123",
            "pi_123/cancel",
            "ca_123/../admin",
            "token_123/delete",
            "token_123\\admin",
            "token_123..admin",
            "token_123?foo=bar",
            "token_123#frag",
            "token_ 123",
        ] {
            assert!(
                matches!(TokenId::new(invalid), Err(MoyasarError::InvalidTokenId)),
                "expected TokenId::new Err for {invalid:?}"
            );
            let json_str = serde_json::to_string(invalid).expect("json string");
            assert!(
                serde_json::from_str::<TokenId>(&json_str).is_err(),
                "expected serde_json::from_str Err for {invalid:?}"
            );
        }
    }

    #[test]
    fn tc_4_1_0_03_masked_card_details_and_no_full_pan() {
        let json = r#"{
            "id": "token_123456",
            "status": "active",
            "number": "XXXX-XXXX-XXXX-1111",
            "company": "visa",
            "gateway_id": "gw_test",
            "created_at": "2026-01-15T10:30:00Z"
        }"#;

        let token: Token = serde_json::from_str(json).expect("valid token payload");
        assert_eq!(token.masked_number(), "XXXX-XXXX-XXXX-1111");
        assert!(token.masked_number().starts_with("XXXX-XXXX-XXXX-"));
        assert_eq!(token.last_four(), Some("1111"));
        assert!(!contains_16_consecutive_digits(token.masked_number()));
    }

    #[test]
    fn tc_4_1_0_05_token_status_transitions_and_lookup() {
        let initiated: TokenStatus = serde_json::from_str(r#""initiated""#).unwrap();
        let active: TokenStatus = serde_json::from_str(r#""active""#).unwrap();
        let inactive: TokenStatus = serde_json::from_str(r#""inactive""#).unwrap();
        let unknown: TokenStatus = serde_json::from_str(r#""suspended""#).unwrap();

        assert_eq!(initiated, TokenStatus::Initiated);
        assert_eq!(active, TokenStatus::Active);
        assert_eq!(inactive, TokenStatus::Inactive);
        assert_eq!(unknown, TokenStatus::Unknown);

        assert!(!initiated.is_active());
        assert!(active.is_active());
        assert!(!inactive.is_active());
        assert!(!unknown.is_active());

        assert_eq!(initiated.as_str(), "initiated");
        assert_eq!(active.as_str(), "active");
        assert_eq!(inactive.as_str(), "inactive");
        assert_eq!(unknown.as_str(), "unknown");
        assert_eq!(active.to_string(), "active");
    }

    #[test]
    fn tc_4_1_0_law_pci_dss_no_pan_or_cvc_retained() {
        let json = r#"{
            "id": "token_law_01",
            "status": "initiated",
            "number": "XXXX-XXXX-XXXX-4242",
            "created_at": "2026-01-15T10:30:00Z"
        }"#;

        let token: Token = serde_json::from_str(json).expect("valid token");
        let debug_output = format!("{token:?}");
        let serialized = serde_json::to_string(&token).expect("serialize token");

        assert!(!contains_16_consecutive_digits(&debug_output));
        assert!(!contains_16_consecutive_digits(&serialized));
        assert!(!serialized.contains("\"cvc\""));
        assert!(!serialized.contains("\"pan\""));
        assert!(token.id.as_str().starts_with(TOKEN_ID_PREFIX));
    }
}
