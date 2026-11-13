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

impl From<&str> for ResponseCode {
    fn from(code: &str) -> Self {
        match code {
            "00" => Self::IssuerApproved,
            "01" => Self::ReferToCardIssuer,
            "03" => Self::InvalidMerchant,
            "05" => Self::DoNotHonor,
            "51" => Self::InsufficientFunds,
            "54" => Self::ExpiredCard,
            "55" => Self::IncorrectPin,
            "61" => Self::ExceedsWithdrawalLimit,
            "62" => Self::RestrictedCard,
            "63" => Self::SecurityViolation,
            "65" => Self::ExceedsFrequencyLimit,
            "91" => Self::IssuerOrSwitchInoperative,
            "96" => Self::SystemMalfunction,
            other => Self::Unknown(other.to_owned()),
        }
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
        let code = match self {
            Self::IssuerApproved => "00",
            Self::ReferToCardIssuer => "01",
            Self::InvalidMerchant => "03",
            Self::DoNotHonor => "05",
            Self::InsufficientFunds => "51",
            Self::ExpiredCard => "54",
            Self::IncorrectPin => "55",
            Self::ExceedsWithdrawalLimit => "61",
            Self::RestrictedCard => "62",
            Self::SecurityViolation => "63",
            Self::ExceedsFrequencyLimit => "65",
            Self::IssuerOrSwitchInoperative => "91",
            Self::SystemMalfunction => "96",
            Self::Unknown(other) => other.as_str(),
        };
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
        assert_eq!(ResponseCode::from("ZZ"), ResponseCode::Unknown("ZZ".into()));
        assert_eq!(ResponseCode::IssuerApproved.to_string(), "00");
    }
}
