//! Minor-unit `Amount`, ISO 4217 `Currency`, and `RecipientType` primitives.

use serde::{Deserialize, Serialize};

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
    pub const fn exponent(self) -> u32 {
        match self {
            Self::JPY => 0,
            Self::KWD | Self::BHD | Self::OMR => 3,
            Self::SAR | Self::USD | Self::AED | Self::QAR | Self::EUR | Self::GBP => 2,
        }
    }
}

/// Monetary amount stored strictly in the smallest currency unit (e.g., Halalas).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Amount(pub i64);

impl Amount {
    /// Constructs an `Amount` directly from minor units.
    pub const fn from_minor(minor: i64) -> Self {
        Self(minor)
    }

    /// Converts a major-unit integer amount into minor units using `currency.exponent()`.
    ///
    /// Returns `None` when the conversion cannot be represented by `i64`.
    pub const fn from_major(major: i64, currency: Currency) -> Option<Self> {
        let factor = 10_i64.pow(currency.exponent());
        match major.checked_mul(factor) {
            Some(minor) => Some(Self(minor)),
            None => None,
        }
    }

    /// Returns the raw integer value in minor units.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_major_to_minor_units_by_currency_exponent() {
        assert_eq!(
            Amount::from_major(1, Currency::SAR)
                .expect("fits")
                .minor_units(),
            100
        );
        assert_eq!(
            Amount::from_major(1, Currency::KWD)
                .expect("fits")
                .minor_units(),
            1000
        );
        assert_eq!(
            Amount::from_major(1, Currency::BHD)
                .expect("fits")
                .minor_units(),
            1000
        );
        assert_eq!(
            Amount::from_major(1, Currency::JPY)
                .expect("fits")
                .minor_units(),
            1
        );
    }

    #[test]
    fn rejects_major_amounts_that_overflow_minor_units() {
        assert!(Amount::from_major(i64::MAX, Currency::SAR).is_none());
    }
}
