//! Field-level validation errors and ISO 8583 acquirer response codes.

use core::fmt;
use serde::{Deserialize, Deserializer};

/// Field-level validation error returned by Moyasar 422 responses.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FieldError {
    /// Name of the invalid request field.
    pub field: String,
    /// Validation failure message.
    pub message: String,
}

/// Two-digit ISO 8583 acquirer response code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseCode {
    /// `"00"` — Approved or completed successfully.
    IssuerApproved,
    /// `"01"` — Refer to card issuer.
    ReferToCardIssuer,
    /// `"03"` — Invalid merchant.
    InvalidMerchant,
    /// `"05"` — Do not honor.
    DoNotHonor,
    /// `"51"` — Insufficient funds.
    InsufficientFunds,
    /// `"54"` — Expired card.
    ExpiredCard,
    /// `"55"` — Incorrect PIN.
    IncorrectPin,
    /// `"61"` — Exceeds withdrawal amount limit.
    ExceedsWithdrawalLimit,
    /// `"62"` — Restricted card.
    RestrictedCard,
    /// `"63"` — Security violation.
    SecurityViolation,
    /// `"65"` — Exceeds withdrawal frequency limit.
    ExceedsFrequencyLimit,
    /// `"91"` — Issuer or switch inoperative.
    IssuerOrSwitchInoperative,
    /// `"96"` — System malfunction.
    SystemMalfunction,
    /// Forward-compatible fallback for unrecognized codes.
    Unknown(String),
}

const KNOWN_CODES: &[(&str, ResponseCode)] = &[
    ("00", ResponseCode::IssuerApproved),
    ("01", ResponseCode::ReferToCardIssuer),
    ("03", ResponseCode::InvalidMerchant),
    ("05", ResponseCode::DoNotHonor),
    ("51", ResponseCode::InsufficientFunds),
    ("54", ResponseCode::ExpiredCard),
    ("55", ResponseCode::IncorrectPin),
    ("61", ResponseCode::ExceedsWithdrawalLimit),
    ("62", ResponseCode::RestrictedCard),
    ("63", ResponseCode::SecurityViolation),
    ("65", ResponseCode::ExceedsFrequencyLimit),
    ("91", ResponseCode::IssuerOrSwitchInoperative),
    ("96", ResponseCode::SystemMalfunction),
];

impl From<&str> for ResponseCode {
    fn from(code: &str) -> Self {
        KNOWN_CODES
            .iter()
            .find(|(k, _)| *k == code)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| Self::Unknown(code.to_owned()))
    }
}

impl<'de> Deserialize<'de> for ResponseCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::from(raw.as_str()))
    }
}

impl fmt::Display for ResponseCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Self::Unknown(other) = self {
            return f.write_str(other);
        }
        let code = KNOWN_CODES
            .iter()
            .find(|(_, v)| v == self)
            .map(|(k, _)| *k)
            .unwrap_or("");
        f.write_str(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_and_unknown_iso8583_codes_without_panicking() {
        assert_eq!(ResponseCode::from("00"), ResponseCode::IssuerApproved);
        assert_eq!(ResponseCode::from("51"), ResponseCode::InsufficientFunds);
        assert_eq!(ResponseCode::from("96"), ResponseCode::SystemMalfunction);
        assert_eq!(ResponseCode::from("ZZ"), ResponseCode::Unknown("ZZ".into()));
        assert_eq!(ResponseCode::IssuerApproved.to_string(), "00");
        assert_eq!(ResponseCode::SystemMalfunction.to_string(), "96");
        assert_eq!(ResponseCode::Unknown("ZZ".into()).to_string(), "ZZ");
    }
}
