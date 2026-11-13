//! Polymorphic `PaymentSourceRequest` and `PaymentSourceResponse` enums (US-2.3.0 Unit 1).

use serde::{Deserialize, Serialize};

use crate::kernel::error::MoyasarError;

use super::source_card::CreditCardSource;
use super::source_digital_wallet::{ApplePaySource, SamsungPaySource};
use super::source_response::{CardSourceResponse, WalletSourceResponse};
use super::source_token::TokenSource;

/// Polymorphic payment source request payload serialized with a lowercase `"type"` discriminator.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::payments::source::PaymentSourceRequest;
/// use some_moyasar::resources::payments::source_token::TokenSource;
///
/// let token = TokenSource::new("token_abc123").expect("valid token");
/// let source = PaymentSourceRequest::Token(token);
/// let json = serde_json::to_value(&source).expect("serializes");
/// assert_eq!(json["type"], "token");
/// ```
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PaymentSourceRequest {
    /// Credit or debit card (Mada, Visa, Mastercard, AMEX, UnionPay).
    CreditCard(CreditCardSource),
    /// Apple Pay digital wallet token.
    ApplePay(ApplePaySource),
    /// Samsung Pay digital wallet token.
    SamsungPay(SamsungPaySource),
    /// Saved card token (`token_...`).
    Token(TokenSource),
}

impl PaymentSourceRequest {
    /// Validates the underlying payment source before sending it over the network.
    pub fn validate(&self) -> Result<(), MoyasarError> {
        match self {
            Self::CreditCard(card) => card.validate(),
            Self::ApplePay(wallet) => wallet.validate(),
            Self::SamsungPay(wallet) => wallet.validate(),
            Self::Token(token) => token.validate(),
        }
    }
}

impl From<CreditCardSource> for PaymentSourceRequest {
    fn from(value: CreditCardSource) -> Self {
        Self::CreditCard(value)
    }
}

impl From<ApplePaySource> for PaymentSourceRequest {
    fn from(value: ApplePaySource) -> Self {
        Self::ApplePay(value)
    }
}

impl From<SamsungPaySource> for PaymentSourceRequest {
    fn from(value: SamsungPaySource) -> Self {
        Self::SamsungPay(value)
    }
}

impl From<TokenSource> for PaymentSourceRequest {
    fn from(value: TokenSource) -> Self {
        Self::Token(value)
    }
}

/// Polymorphic payment source response returned by the Moyasar API.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PaymentSourceResponse {
    /// Credit or debit card response metadata.
    CreditCard(CardSourceResponse),
    /// Apple Pay digital wallet response metadata.
    ApplePay(WalletSourceResponse),
    /// Samsung Pay digital wallet response metadata.
    SamsungPay(WalletSourceResponse),
    /// Saved card token response metadata.
    Token(CardSourceResponse),
    /// Forward-compatible fallback for unrecognized or future source types.
    #[serde(other)]
    Unknown,
}
