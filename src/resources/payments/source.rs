//! Polymorphic `PaymentSourceRequest` and `PaymentSourceResponse` enums (US-2.3.0 Unit 1).

use serde::{Deserialize, Serialize};

use crate::kernel::error::MoyasarError;

use super::source_card::CreditCardSource;
use super::source_digital_wallet::{ApplePaySource, SamsungPaySource};
use super::source_response::{CardSourceResponse, WalletSourceResponse};
use super::source_sadad::{SadadBillSource, SadadBillSourceResponse};
use super::source_stcpay::{StcPaySource, StcPaySourceResponse, normalize_saudi_mobile};
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
    /// STC Pay Saudi mobile wallet (`stcpay`).
    StcPay(StcPaySource),
    /// SADAD bill payment (`sadad` / `sadadbill`).
    #[serde(rename = "sadad", alias = "sadadbill")]
    SadadBill(SadadBillSource),
}

impl PaymentSourceRequest {
    /// Validates the underlying payment source before sending it over the network.
    pub fn validate(&self) -> Result<(), MoyasarError> {
        match self {
            Self::CreditCard(card) => card.validate(),
            Self::ApplePay(wallet) => wallet.validate(),
            Self::SamsungPay(wallet) => wallet.validate(),
            Self::Token(token) => token.validate(),
            Self::StcPay(stcpay) => normalize_saudi_mobile(&stcpay.mobile).map(|_| ()),
            Self::SadadBill(sadad) => sadad.validate_utc(),
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

impl From<StcPaySource> for PaymentSourceRequest {
    fn from(value: StcPaySource) -> Self {
        Self::StcPay(value)
    }
}

impl From<SadadBillSource> for PaymentSourceRequest {
    fn from(value: SadadBillSource) -> Self {
        Self::SadadBill(value)
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
    /// STC Pay response metadata (`stcpay`).
    StcPay(StcPaySourceResponse),
    /// SADAD bill response metadata (`sadad` or `sadadbill`).
    #[serde(rename = "sadad", alias = "sadadbill")]
    SadadBill(SadadBillSourceResponse),
    /// Forward-compatible fallback for unrecognized or future source types.
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serializes_and_validates_stcpay_and_sadad_sources() {
        let stc = StcPaySource::new("0515555555").expect("valid mobile");
        let req_stc = PaymentSourceRequest::from(stc);
        req_stc.validate().expect("valid stcpay request");
        let json_stc = serde_json::to_value(&req_stc).expect("serializes");
        assert_eq!(json_stc["type"], "stcpay");
        assert_eq!(json_stc["mobile"], "0515555555");

        let sadad = SadadBillSource::new().with_biller_code("007");
        let req_sadad = PaymentSourceRequest::from(sadad);
        req_sadad.validate().expect("valid sadad request");
        let json_sadad = serde_json::to_value(&req_sadad).expect("serializes");
        assert_eq!(json_sadad["type"], "sadad");
        assert_eq!(json_sadad["biller_code"], "007");
    }

    #[test]
    fn deserializes_stcpay_and_sadad_response_variants_with_alias() {
        let stc_resp: PaymentSourceResponse = serde_json::from_value(json!({
            "type": "stcpay",
            "mobile": "0515555555",
            "transaction_url": "https://api.moyasar.com/v1/stc_pays/1/proceed?otp_token=abc"
        }))
        .expect("deserializes stcpay response");
        assert!(matches!(stc_resp, PaymentSourceResponse::StcPay(_)));

        let sadad_resp: PaymentSourceResponse = serde_json::from_value(json!({
            "type": "sadad",
            "biller_code": "007",
            "sadad_number": "900012345678"
        }))
        .expect("deserializes sadad response");
        assert!(matches!(sadad_resp, PaymentSourceResponse::SadadBill(_)));

        let sadadbill_resp: PaymentSourceResponse = serde_json::from_value(json!({
            "type": "sadadbill",
            "biller_code": "901",
            "sadad_number": "900087654321"
        }))
        .expect("deserializes sadadbill alias response");
        assert!(matches!(
            sadadbill_resp,
            PaymentSourceResponse::SadadBill(_)
        ));
    }
}
