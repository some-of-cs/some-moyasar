//! Client-side card validation: Luhn checksum, expiry date, and brand CVC length (US-2.3.0 Unit 5).

use chrono::{Datelike, Utc};

use crate::kernel::error::MoyasarError;

/// Validates a card PAN using the Luhn checksum without heap allocation.
///
/// Accepts ASCII digits (ignoring spaces and hyphens) with length between 12 and 19.
pub fn luhn_valid(number: &str) -> bool {
    let mut count = 0usize;
    let mut sum = 0u32;
    for byte in number.bytes().rev() {
        if byte == b' ' || byte == b'-' {
            continue;
        }
        if !byte.is_ascii_digit() {
            return false;
        }
        let digit = u32::from(byte - b'0');
        let doubled = if count % 2 == 1 { digit * 2 } else { digit };
        sum += if doubled > 9 { doubled - 9 } else { doubled };
        count += 1;
    }
    (12..=19).contains(&count) && sum % 10 == 0
}

/// Returns `true` if the PAN begins with an American Express prefix (`34` or `37`).
pub fn is_amex(number: &str) -> bool {
    let mut digits = number.bytes().filter(u8::is_ascii_digit);
    matches!((digits.next(), digits.next()), (Some(b'3'), Some(b'4' | b'7')))
}

/// Validates CVC digit length against card brand (4 digits for AMEX, 3 digits otherwise).
///
/// # Errors
///
/// Returns [`MoyasarError::InvalidCvc`] if the CVC contains non-digits or has the wrong length.
pub fn validate_cvc(number: &str, cvc: &str) -> Result<(), MoyasarError> {
    if !cvc.bytes().all(|b| b.is_ascii_digit()) {
        return Err(MoyasarError::InvalidCvc);
    }
    let expected = if is_amex(number) { 4 } else { 3 };
    if cvc.len() != expected {
        return Err(MoyasarError::InvalidCvc);
    }
    Ok(())
}

/// Validates a card expiration `(month, year)` against an explicit `(current_year, current_month)`.
///
/// Two-digit years are normalized to `2000 + year`.
///
/// # Errors
///
/// Returns [`MoyasarError::CardExpired`] if `month` is outside `1..=12` or the date is in the past.
pub fn validate_expiry_against(
    month: u8,
    year: u16,
    current_year: i32,
    current_month: u32,
) -> Result<(), MoyasarError> {
    if !(1..=12).contains(&month) {
        return Err(MoyasarError::CardExpired);
    }
    let full_year = if year < 100 {
        2000 + i32::from(year)
    } else {
        i32::from(year)
    };
    if full_year < current_year || (full_year == current_year && u32::from(month) < current_month) {
        return Err(MoyasarError::CardExpired);
    }
    Ok(())
}

/// Validates a card expiration `(month, year)` against the current UTC date.
///
/// # Errors
///
/// Returns [`MoyasarError::CardExpired`] if the card is expired or `month` is invalid.
pub fn validate_expiry(month: u8, year: u16) -> Result<(), MoyasarError> {
    let now = Utc::now();
    validate_expiry_against(month, year, now.year(), now.month())
}

/// Runs full client-side card validation: Luhn checksum, expiry date, and brand-aware CVC length.
///
/// # Errors
///
/// Returns [`MoyasarError::InvalidCardNumber`], [`MoyasarError::CardExpired`], or
/// [`MoyasarError::InvalidCvc`].
pub fn validate_card_fields(
    number: &str,
    month: u8,
    year: u16,
    cvc: &str,
) -> Result<(), MoyasarError> {
    if !luhn_valid(number) {
        return Err(MoyasarError::InvalidCardNumber);
    }
    validate_expiry(month, year)?;
    validate_cvc(number, cvc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_2_3_0_02_luhn_table_valid_and_invalid() {
        let valid = [
            "4111111111111111",
            "4242424242424242",
            "5555555555554444",
            "378282246310005",
            "371449635398431",
            "6011111111111117",
            "3530111333300000",
            "30569309025904",
            "4012888888881881",
            "5105105105105100",
        ];
        let invalid = [
            "4111111111111112",
            "4242424242424243",
            "5555555555554445",
            "378282246310006",
            "371449635398432",
            "6011111111111118",
            "1234",
            "411111111111111a",
            "",
            "00000000000",
        ];
        for pan in valid {
            assert!(luhn_valid(pan), "expected valid Luhn for {pan}");
        }
        for pan in invalid {
            assert!(!luhn_valid(pan), "expected invalid Luhn for {pan}");
        }
    }

    #[test]
    fn tc_2_3_0_03_brand_aware_cvc_length() {
        assert!(validate_cvc("378282246310005", "1234").is_ok());
        assert!(matches!(
            validate_cvc("378282246310005", "123"),
            Err(MoyasarError::InvalidCvc)
        ));
        assert!(validate_cvc("4111111111111111", "123").is_ok());
        assert!(matches!(
            validate_cvc("4111111111111111", "1234"),
            Err(MoyasarError::InvalidCvc)
        ));
    }

    #[test]
    fn tc_2_3_0_04_expiry_validation_boundaries() {
        assert!(matches!(
            validate_expiry_against(12, 2020, 2026, 11),
            Err(MoyasarError::CardExpired)
        ));
        assert!(matches!(
            validate_expiry_against(10, 2026, 2026, 11),
            Err(MoyasarError::CardExpired)
        ));
        assert!(validate_expiry_against(11, 2026, 2026, 11).is_ok());
        assert!(validate_expiry_against(12, 30, 2026, 11).is_ok());
    }
}
