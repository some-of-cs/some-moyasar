//! Saudi IBAN validation helpers (`SA` + 22 ASCII digits).

use crate::kernel::error::MoyasarError;
pub use crate::resources::payouts::accounts_retrieve::validate_sa_iban;

/// Validates that `iban` is a syntactically valid 24-character Saudi IBAN (`SA` + 22 ASCII digits).
///
/// # Errors
///
/// Returns [`MoyasarError::InvalidIban`] if `iban` does not start with `"SA"`, is not 24 bytes
/// long, or contains any non-ASCII-digit character after `"SA"`.
///
/// # Examples
///
/// ```
/// use some_moyasar::kernel::error::MoyasarError;
/// use some_moyasar::resources::payouts::iban::validate_saudi_iban;
///
/// assert!(validate_saudi_iban("SA0380000000608010167519").is_ok());
/// assert!(matches!(
///     validate_saudi_iban("SA12345"),
///     Err(MoyasarError::InvalidIban)
/// ));
/// ```
pub fn validate_saudi_iban(iban: &str) -> Result<(), MoyasarError> {
    validate_sa_iban(iban)
}
