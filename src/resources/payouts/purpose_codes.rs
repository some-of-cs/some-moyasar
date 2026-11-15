//! Re-exports and type alias for SAMA-regulated payout [`PurposeCode`]s.

pub use crate::resources::payouts::purpose::{
    KNOWN_PURPOSES, ParsePayoutPurposeError, PayoutPurpose,
};

/// Type alias for [`PayoutPurpose`] representing the 20 SAMA-regulated payout purpose codes.
///
/// # Examples
///
/// ```
/// use std::str::FromStr;
/// use some_moyasar::resources::payouts::purpose_codes::PurposeCode;
///
/// let code = PurposeCode::from_str("payment_to_merchant").unwrap();
/// assert_eq!(code, PurposeCode::PaymentToMerchant);
/// assert_eq!(code.as_str(), "payment_to_merchant");
/// ```
pub type PurposeCode = PayoutPurpose;
