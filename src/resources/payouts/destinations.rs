//! Payout destination models and validators (`US-5.2.0` Unit 3).

use serde::{Deserialize, Serialize};

use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::response_code::FieldError;

/// Bank account destination for a Moyasar payout disbursement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankDestination {
    /// Beneficiary Saudi IBAN (`SA` followed by 22 ASCII digits).
    pub iban: String,
    /// Legal name of the beneficiary / payee (required for SAMA compliance).
    pub name: String,
    /// Beneficiary Saudi mobile number (`05XXXXXXXX`, `+9665XXXXXXXX`, or `9665XXXXXXXX`).
    pub mobile: String,
    /// Two-letter ISO 3166-1 alpha-2 destination country code (e.g., `"SA"`).
    pub country: String,
    /// Beneficiary city name (e.g., `"Riyadh"`).
    pub city: String,
}

/// Mobile wallet destination for a Moyasar payout disbursement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletDestination {
    /// Beneficiary Saudi mobile number (`05XXXXXXXX`, `+9665XXXXXXXX`, or `9665XXXXXXXX`).
    pub mobile: String,
}

/// Polymorphic payout destination tagged by `"type"` (`"bank"` or `"wallet"`).
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::payouts::destinations::{
///     BankDestination, PayoutDestination, WalletDestination,
/// };
///
/// let bank = PayoutDestination::Bank(BankDestination {
///     iban: "SA0380000000608010167519".to_owned(),
///     name: "Acme Logistics LLC".to_owned(),
///     mobile: "0512345678".to_owned(),
///     country: "SA".to_owned(),
///     city: "Riyadh".to_owned(),
/// });
/// assert!(bank.validate().is_ok());
///
/// let wallet = PayoutDestination::Wallet(WalletDestination {
///     mobile: "+966512345678".to_owned(),
/// });
/// assert!(wallet.validate().is_ok());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PayoutDestination {
    /// Bank transfer destination (internal, IPS, or SARIE).
    Bank(BankDestination),
    /// Mobile wallet transfer destination.
    Wallet(WalletDestination),
}

impl PayoutDestination {
    /// Validates the destination fields according to SAMA and Moyasar rules.
    ///
    /// - For [`PayoutDestination::Bank`]: validates `iban` via [`validate_sa_iban`],
    ///   ensures `name.trim()` is non-empty (returning [`ApiErrorType::InvalidRequestError`]),
    ///   and validates `mobile` via [`validate_sa_mobile`].
    /// - For [`PayoutDestination::Wallet`]: validates `mobile` via [`validate_sa_mobile`].
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::InvalidIban`], [`MoyasarError::InvalidMobile`], or
    /// [`MoyasarError::Api`] with [`ApiErrorType::InvalidRequestError`] on validation failure.
    pub fn validate(&self) -> Result<(), MoyasarError> {
        match self {
            Self::Bank(d) => {
                validate_sa_iban(&d.iban)?;
                if d.name.trim().is_empty() {
                    return Err(MoyasarError::Api {
                        error_type: ApiErrorType::InvalidRequestError,
                        message: "payee name must not be empty".to_owned(),
                        errors: vec![FieldError {
                            field: "name".to_owned(),
                            message: "payee name must not be empty".to_owned(),
                        }],
                        response_code: None,
                    });
                }
                validate_sa_mobile(&d.mobile)
            }
            Self::Wallet(d) => validate_sa_mobile(&d.mobile),
        }
    }
}

/// Validates that `iban` is a syntactically valid Saudi IBAN (`SA` followed by 22 ASCII digits).
///
/// # Errors
///
/// Returns [`MoyasarError::InvalidIban`] if `iban` does not start with `"SA"`, is not 24 bytes
/// long, or contains non-digit characters after the `"SA"` prefix.
pub fn validate_sa_iban(iban: &str) -> Result<(), MoyasarError> {
    let ok =
        iban.starts_with("SA") && iban.len() == 24 && iban[2..].chars().all(|c| c.is_ascii_digit());
    if ok {
        Ok(())
    } else {
        Err(MoyasarError::InvalidIban)
    }
}

/// Validates that `mobile` matches an accepted Saudi mobile number format.
///
/// Accepted formats:
/// - `05` followed by 8 ASCII digits (10 characters)
/// - `+9665` followed by 8 ASCII digits (13 characters)
/// - `9665` followed by 8 ASCII digits (12 characters)
///
/// # Errors
///
/// Returns [`MoyasarError::InvalidMobile`] if `mobile` does not match any accepted format.
pub fn validate_sa_mobile(mobile: &str) -> Result<(), MoyasarError> {
    let rest = mobile
        .strip_prefix("+9665")
        .or_else(|| mobile.strip_prefix("9665"))
        .or_else(|| mobile.strip_prefix("05"));
    match rest {
        Some(digits) if digits.len() == 8 && digits.bytes().all(|b| b.is_ascii_digit()) => Ok(()),
        _ => Err(MoyasarError::InvalidMobile),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_5_2_0_04_wallet_destination_serializes_with_type_tag() {
        let wallet = PayoutDestination::Wallet(WalletDestination {
            mobile: "0512345678".to_owned(),
        });

        let json = serde_json::to_value(&wallet).expect("wallet destination serializes");
        assert_eq!(json["type"], "wallet");
        assert_eq!(json["mobile"], "0512345678");

        let decoded: PayoutDestination =
            serde_json::from_value(json).expect("wallet destination deserializes");
        assert_eq!(decoded, wallet);

        let bank = PayoutDestination::Bank(BankDestination {
            iban: "SA0380000000608010167519".to_owned(),
            name: "Acme Logistics LLC".to_owned(),
            mobile: "+966512345678".to_owned(),
            country: "SA".to_owned(),
            city: "Riyadh".to_owned(),
        });
        let bank_json = serde_json::to_value(&bank).expect("bank destination serializes");
        assert_eq!(bank_json["type"], "bank");
        assert_eq!(bank_json["iban"], "SA0380000000608010167519");
        assert_eq!(bank_json["name"], "Acme Logistics LLC");
        assert_eq!(bank_json["mobile"], "+966512345678");
        assert_eq!(bank_json["country"], "SA");
        assert_eq!(bank_json["city"], "Riyadh");
    }

    #[test]
    fn tc_5_2_0_law_validates_iban_payee_name_and_mobile() {
        // Valid Saudi mobile formats
        for valid in ["0512345678", "+966512345678", "966512345678"] {
            assert!(
                validate_sa_mobile(valid).is_ok(),
                "expected {valid} to be valid"
            );
        }

        // Invalid Saudi mobile formats
        for invalid in [
            "",
            "0612345678",
            "051234567",
            "05123456789",
            "051234567a",
            "+966412345678",
            "966412345678",
            "00966512345678",
        ] {
            assert!(
                matches!(
                    validate_sa_mobile(invalid),
                    Err(MoyasarError::InvalidMobile)
                ),
                "expected {invalid} to fail with InvalidMobile"
            );
        }

        // Valid and invalid Saudi IBANs
        assert!(validate_sa_iban("SA0380000000608010167519").is_ok());
        for bad_iban in [
            "US0380000000608010167519",
            "SA038000000060801016751",
            "SA03800000006080101675199",
            "SA038000000060801016751A",
        ] {
            assert!(
                matches!(validate_sa_iban(bad_iban), Err(MoyasarError::InvalidIban)),
                "expected {bad_iban} to fail with InvalidIban"
            );
        }

        // BankDestination validation: payee name must be non-empty after trim
        let blank_name = PayoutDestination::Bank(BankDestination {
            iban: "SA0380000000608010167519".to_owned(),
            name: "   ".to_owned(),
            mobile: "0512345678".to_owned(),
            country: "SA".to_owned(),
            city: "Riyadh".to_owned(),
        });
        assert!(matches!(
            blank_name.validate(),
            Err(MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            })
        ));

        // BankDestination validation: invalid IBAN & invalid mobile
        let bad_iban_bank = PayoutDestination::Bank(BankDestination {
            iban: "SA123".to_owned(),
            name: "Acme".to_owned(),
            mobile: "0512345678".to_owned(),
            country: "SA".to_owned(),
            city: "Riyadh".to_owned(),
        });
        assert!(matches!(
            bad_iban_bank.validate(),
            Err(MoyasarError::InvalidIban)
        ));

        let bad_mobile_bank = PayoutDestination::Bank(BankDestination {
            iban: "SA0380000000608010167519".to_owned(),
            name: "Acme".to_owned(),
            mobile: "12345".to_owned(),
            country: "SA".to_owned(),
            city: "Riyadh".to_owned(),
        });
        assert!(matches!(
            bad_mobile_bank.validate(),
            Err(MoyasarError::InvalidMobile)
        ));

        // WalletDestination validation
        let valid_wallet = PayoutDestination::Wallet(WalletDestination {
            mobile: "966512345678".to_owned(),
        });
        assert!(valid_wallet.validate().is_ok());

        let invalid_wallet = PayoutDestination::Wallet(WalletDestination {
            mobile: "0612345678".to_owned(),
        });
        assert!(matches!(
            invalid_wallet.validate(),
            Err(MoyasarError::InvalidMobile)
        ));
    }
}
