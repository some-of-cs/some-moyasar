//! SAMA-regulated 20-variant `PayoutPurpose` enum (`US-5.2.0` Unit 2).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// SAMA-regulated payout purpose codes required for disbursement requests.
///
/// Each variant serializes and deserializes strictly as its `snake_case`
/// identifier without an `#[serde(other)]` fallback, ensuring unrecognized
/// purpose codes fail fast at deserialization or parsing time.
///
/// # Examples
///
/// ```
/// use std::str::FromStr;
/// use some_moyasar::resources::payouts::purpose::PayoutPurpose;
///
/// let purpose = PayoutPurpose::from_str("payroll_benefits").unwrap();
/// assert_eq!(purpose, PayoutPurpose::PayrollBenefits);
/// assert_eq!(purpose.as_str(), "payroll_benefits");
/// assert_eq!(purpose.to_string(), "payroll_benefits");
///
/// let json = serde_json::to_string(&purpose).unwrap();
/// assert_eq!(json, "\"payroll_benefits\"");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayoutPurpose {
    /// Utility bills, rent, or lease payments (`bills_or_rent`).
    BillsOrRent,
    /// General expenses and service fees (`expenses_services`).
    ExpensesServices,
    /// Purchase of physical or financial assets (`purchase_assets`).
    PurchaseAssets,
    /// Savings deposits or investment contributions (`saving_investment`).
    SavingInvestment,
    /// Taxes, fines, or other government dues (`government_dues`).
    GovernmentDues,
    /// Currency exchange and remittance settlements (`money_exchange`).
    MoneyExchange,
    /// Credit card or loan repayments (`credit_card_loan`).
    CreditCardLoan,
    /// Gifts, bonuses, or reward disbursements (`gift_or_reward`).
    GiftOrReward,
    /// Personal transfers and individual expenses (`personal`).
    Personal,
    /// Capital market or investment transactions (`investment_transaction`).
    InvestmentTransaction,
    /// Financial support and family assistance (`family_assistance`).
    FamilyAssistance,
    /// Charitable donations and non-profit contributions (`donation`).
    Donation,
    /// Employee salaries, payroll, and benefits (`payroll_benefits`).
    PayrollBenefits,
    /// E-commerce and online retail purchases (`online_purchase`).
    OnlinePurchase,
    /// Hajj and Umrah travel or pilgrimage expenses (`hajj_and_umra`).
    HajjAndUmra,
    /// Shareholder dividend distributions (`dividend_payment`).
    DividendPayment,
    /// Direct payments to government entities (`government_payment`).
    GovernmentPayment,
    /// Real estate or investment house transactions (`investment_house`).
    InvestmentHouse,
    /// Commercial settlements to merchants (`payment_to_merchant`).
    PaymentToMerchant,
    /// Transfers between accounts owned by the same beneficiary (`own_account_transfer`).
    OwnAccountTransfer,
}

/// Static lookup table mapping all 20 SAMA-regulated `snake_case` purpose
/// codes to their corresponding [`PayoutPurpose`] variants.
pub const KNOWN_PURPOSES: &[(&str, PayoutPurpose)] = &[
    ("bills_or_rent", PayoutPurpose::BillsOrRent),
    ("expenses_services", PayoutPurpose::ExpensesServices),
    ("purchase_assets", PayoutPurpose::PurchaseAssets),
    ("saving_investment", PayoutPurpose::SavingInvestment),
    ("government_dues", PayoutPurpose::GovernmentDues),
    ("money_exchange", PayoutPurpose::MoneyExchange),
    ("credit_card_loan", PayoutPurpose::CreditCardLoan),
    ("gift_or_reward", PayoutPurpose::GiftOrReward),
    ("personal", PayoutPurpose::Personal),
    (
        "investment_transaction",
        PayoutPurpose::InvestmentTransaction,
    ),
    ("family_assistance", PayoutPurpose::FamilyAssistance),
    ("donation", PayoutPurpose::Donation),
    ("payroll_benefits", PayoutPurpose::PayrollBenefits),
    ("online_purchase", PayoutPurpose::OnlinePurchase),
    ("hajj_and_umra", PayoutPurpose::HajjAndUmra),
    ("dividend_payment", PayoutPurpose::DividendPayment),
    ("government_payment", PayoutPurpose::GovernmentPayment),
    ("investment_house", PayoutPurpose::InvestmentHouse),
    ("payment_to_merchant", PayoutPurpose::PaymentToMerchant),
    ("own_account_transfer", PayoutPurpose::OwnAccountTransfer),
];

impl PayoutPurpose {
    /// Returns the SAMA-regulated `snake_case` string representation of this purpose.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        KNOWN_PURPOSES
            .iter()
            .find(|(_, variant)| variant == self)
            .map_or("", |(code, _)| *code)
    }
}

impl fmt::Display for PayoutPurpose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned when parsing a [`PayoutPurpose`] from an unrecognized string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsePayoutPurposeError {
    input: String,
}

impl ParsePayoutPurposeError {
    fn new(input: &str) -> Self {
        Self {
            input: input.to_owned(),
        }
    }

    /// Returns the unrecognized input string that failed to parse.
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }
}

impl fmt::Display for ParsePayoutPurposeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown payout purpose code: {:?}", self.input)
    }
}

impl std::error::Error for ParsePayoutPurposeError {}

impl FromStr for PayoutPurpose {
    type Err = ParsePayoutPurposeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        KNOWN_PURPOSES
            .iter()
            .find(|(code, _)| *code == s)
            .map(|(_, variant)| *variant)
            .ok_or_else(|| ParsePayoutPurposeError::new(s))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::str::FromStr;

    use super::{KNOWN_PURPOSES, ParsePayoutPurposeError, PayoutPurpose};

    #[test]
    fn tc_5_2_0_02_all_20_variants_round_trip_and_invalid_fail() {
        assert_eq!(KNOWN_PURPOSES.len(), 20);

        let mut unique_codes = HashSet::new();
        let mut unique_variants = HashSet::new();

        for &(expected_code, expected_variant) in KNOWN_PURPOSES {
            assert!(
                unique_codes.insert(expected_code),
                "duplicate code in KNOWN_PURPOSES: {expected_code}"
            );
            assert!(
                unique_variants.insert(expected_variant),
                "duplicate variant in KNOWN_PURPOSES: {expected_variant:?}"
            );

            // as_str & Display
            assert_eq!(expected_variant.as_str(), expected_code);
            assert_eq!(expected_variant.to_string(), expected_code);

            // FromStr
            let parsed = PayoutPurpose::from_str(expected_code)
                .expect("known purpose code must parse via FromStr");
            assert_eq!(parsed, expected_variant);

            // Serde serialize & deserialize
            let quoted = format!("\"{expected_code}\"");
            let serialized = serde_json::to_string(&expected_variant)
                .expect("PayoutPurpose must serialize to snake_case JSON string");
            assert_eq!(serialized, quoted);

            let deserialized: PayoutPurpose = serde_json::from_str(&quoted)
                .expect("known purpose code must deserialize from JSON string");
            assert_eq!(deserialized, expected_variant);
        }

        // Invalid strings must fail both FromStr and serde_json::from_str (no #[serde(other)])
        let invalid_inputs = [
            "",
            "unknown",
            "other",
            "PAYROLL_BENEFITS",
            "billsOrRent",
            "bills-or-rent",
            "personal ",
        ];

        for invalid in invalid_inputs {
            let err = PayoutPurpose::from_str(invalid)
                .expect_err("invalid purpose code must fail FromStr");
            assert_eq!(err.input(), invalid);
            assert!(!err.to_string().is_empty());
            let _: &dyn std::error::Error = &err;

            let quoted = format!("\"{invalid}\"");
            assert!(
                serde_json::from_str::<PayoutPurpose>(&quoted).is_err(),
                "invalid purpose code {invalid:?} must fail serde deserialization"
            );
        }

        let err_clone = ParsePayoutPurposeError::new("bad_code");
        assert_eq!(err_clone.input(), "bad_code");
    }
}
