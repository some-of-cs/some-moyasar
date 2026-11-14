//! Minor-unit `Amount`, ISO 4217 `Currency`, `RecipientType`, and URL path-segment validators.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::kernel::error::MoyasarError;

const CURRENCY_CODES: &[(&str, Currency)] = &[
    ("SAR", Currency::SAR),
    ("USD", Currency::USD),
    ("KWD", Currency::KWD),
    ("BHD", Currency::BHD),
    ("AED", Currency::AED),
    ("QAR", Currency::QAR),
    ("OMR", Currency::OMR),
    ("EUR", Currency::EUR),
    ("GBP", Currency::GBP),
    ("JPY", Currency::JPY),
];

/// ISO 4217 currency codes with minor-unit exponent awareness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Currency {
    /// Saudi Riyal (2 decimal places: 1 SAR = 100 Halalas).
    SAR,
    /// US Dollar (2 decimal places: 1 USD = 100 Cents).
    USD,
    /// Kuwaiti Dinar (3 decimal places: 1 KWD = 1000 Fils).
    KWD,
    /// Bahraini Dinar (3 decimal places: 1 BHD = 1000 Fils).
    BHD,
    /// UAE Dirham (2 decimal places).
    AED,
    /// Qatari Riyal (2 decimal places).
    QAR,
    /// Omani Rial (3 decimal places).
    OMR,
    /// Euro (2 decimal places).
    EUR,
    /// British Pound (2 decimal places).
    GBP,
    /// Japanese Yen (0 decimal places).
    JPY,
}

impl Currency {
    /// Returns the number of decimal places for this currency.
    #[must_use]
    pub const fn exponent(self) -> u32 {
        match self {
            Self::JPY => 0,
            Self::KWD | Self::BHD | Self::OMR => 3,
            Self::SAR | Self::USD | Self::AED | Self::QAR | Self::EUR | Self::GBP => 2,
        }
    }

    /// Returns the canonical 3-letter ISO 4217 code for this currency.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SAR => "SAR",
            Self::USD => "USD",
            Self::KWD => "KWD",
            Self::BHD => "BHD",
            Self::AED => "AED",
            Self::QAR => "QAR",
            Self::OMR => "OMR",
            Self::EUR => "EUR",
            Self::GBP => "GBP",
            Self::JPY => "JPY",
        }
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Currency {
    type Err = MoyasarError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let upper = raw.trim().to_ascii_uppercase();
        CURRENCY_CODES
            .iter()
            .find(|(code, _)| *code == upper)
            .map(|(_, cur)| *cur)
            .ok_or_else(|| MoyasarError::invalid_request(format!("unsupported currency: {raw}")))
    }
}

/// Monetary amount stored strictly in the smallest currency unit (e.g., Halalas).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Amount(pub i64);

impl Amount {
    /// Constructs an `Amount` directly from minor units.
    #[must_use]
    pub const fn from_minor(minor: i64) -> Self {
        Self(minor)
    }

    /// Converts a major-unit integer amount into minor units using `currency.exponent()`.
    #[must_use]
    pub const fn from_major(major: i64, currency: Currency) -> Self {
        let factor = 10_i64.pow(currency.exponent());
        Self(major * factor)
    }

    /// Returns the raw integer value in minor units.
    #[must_use]
    pub const fn minor_units(self) -> i64 {
        self.0
    }
}

/// Payout recipient classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipientType {
    /// Individual person.
    Individual,
    /// Registered business or organization.
    Organization,
}

/// Returns `true` when `c` is a safe resource identifier character (not whitespace or `/?#\`).
#[must_use]
pub fn is_valid_id_char(c: char) -> bool {
    !c.is_whitespace() && !"/?#\\".contains(c)
}

/// Returns `true` when `segment` is a non-empty, URL-safe path segment without `..` traversal.
#[must_use]
pub fn is_valid_path_segment(segment: &str) -> bool {
    !segment.is_empty() && !segment.contains("..") && segment.chars().all(is_valid_id_char)
}

/// Returns `true` when `raw` starts with `prefix` followed by a valid URL-safe suffix.
#[must_use]
pub fn is_valid_prefixed_id(raw: &str, prefix: &str) -> bool {
    raw.strip_prefix(prefix).is_some_and(is_valid_path_segment)
}

/// Validates that `raw` is a non-empty, URL-safe path segment, returning an `InvalidRequestError`
/// with `label` when invalid.
pub fn validate_path_segment(raw: &str, label: &str) -> Result<(), MoyasarError> {
    if !is_valid_path_segment(raw) {
        return Err(MoyasarError::invalid_request(format!(
            "{label} must be a non-empty URL-safe path segment"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_major_to_minor_units_by_currency_exponent() {
        assert_eq!(Amount::from_major(1, Currency::SAR).minor_units(), 100);
        assert_eq!(Amount::from_major(1, Currency::KWD).minor_units(), 1000);
        assert_eq!(Amount::from_major(1, Currency::BHD).minor_units(), 1000);
        assert_eq!(Amount::from_major(1, Currency::JPY).minor_units(), 1);
    }

    #[test]
    fn currency_display_and_from_str_roundtrip() {
        for (code, expected) in CURRENCY_CODES {
            assert_eq!(expected.to_string(), *code);
            assert_eq!(code.parse::<Currency>().unwrap(), *expected);
            assert_eq!(code.to_lowercase().parse::<Currency>().unwrap(), *expected);
        }
        assert!("XYZ".parse::<Currency>().is_err());
    }

    #[test]
    fn validates_path_segments_and_prefixed_ids() {
        assert!(is_valid_path_segment("pay_123"));
        assert!(!is_valid_path_segment(""));
        assert!(!is_valid_path_segment("../admin"));
        assert!(!is_valid_path_segment("a/b"));
        assert!(is_valid_prefixed_id("token_abc", "token_"));
        assert!(!is_valid_prefixed_id("token_", "token_"));
    }
}
