//! Typed payment source response models and card brand classification (US-2.3.0 Unit 6).

use core::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::kernel::response_code::ResponseCode;

/// Card network / scheme brand returned in Moyasar payment source responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardBrand {
    /// Saudi domestic payment network (`"mada"`).
    Mada,
    /// Visa (`"visa"`).
    Visa,
    /// Mastercard (`"master"` / `"mastercard"`).
    Mastercard,
    /// American Express (`"amex"`).
    Amex,
    /// UnionPay (`"unionpay"`).
    UnionPay,
    /// Forward-compatible fallback for unrecognized card brands.
    Unknown(String),
}

const KNOWN_BRANDS: &[(&str, CardBrand)] = &[
    ("mada", CardBrand::Mada),
    ("visa", CardBrand::Visa),
    ("master", CardBrand::Mastercard),
    ("mastercard", CardBrand::Mastercard),
    ("amex", CardBrand::Amex),
    ("unionpay", CardBrand::UnionPay),
];

impl From<&str> for CardBrand {
    fn from(raw: &str) -> Self {
        let lower = raw.to_ascii_lowercase();
        KNOWN_BRANDS
            .iter()
            .find(|(k, _)| *k == lower.as_str())
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| Self::Unknown(raw.to_owned()))
    }
}

impl fmt::Display for CardBrand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Self::Unknown(other) = self {
            return f.write_str(other);
        }
        let wire = KNOWN_BRANDS
            .iter()
            .find(|(_, v)| v == self)
            .map(|(k, _)| *k)
            .unwrap_or("unknown");
        f.write_str(wire)
    }
}

impl Serialize for CardBrand {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for CardBrand {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::from(raw.as_str()))
    }
}

/// Issuer card funding classification (`debit`, `credit`, `prepaid`, `charge`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssuerCardType {
    /// Debit card.
    Debit,
    /// Credit card.
    Credit,
    /// Prepaid card.
    Prepaid,
    /// Charge card.
    Charge,
    /// Forward-compatible fallback for unrecognized card types.
    #[serde(other)]
    Unknown,
}

fn serialize_opt_response_code<S: Serializer>(
    code: &Option<ResponseCode>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match code {
        Some(c) => serializer.serialize_some(&c.to_string()),
        None => serializer.serialize_none(),
    }
}

/// Credit or debit card source metadata returned by the Moyasar Payments API.
///
/// Only the masked card `number` is ever retained (satisfying PCI-DSS 4.0 Requirement 3.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardSourceResponse {
    /// Card network / scheme (`mada`, `visa`, `master`, `amex`, etc.).
    #[serde(default)]
    pub company: Option<CardBrand>,
    /// Cardholder name.
    #[serde(default)]
    pub name: Option<String>,
    /// Masked PAN (e.g., `"4111-XXXX-XXXX-1111"`).
    #[serde(default)]
    pub number: Option<String>,
    /// Acquirer gateway transaction identifier.
    #[serde(default)]
    pub gateway_id: Option<String>,
    /// Bank reference number (RRN).
    #[serde(default)]
    pub reference_number: Option<String>,
    /// Reusable saved card token (`token_...`) populated when `save_card: true`.
    #[serde(default)]
    pub token: Option<String>,
    /// Human-readable gateway status message.
    #[serde(default)]
    pub message: Option<String>,
    /// 3-D Secure challenge redirect URL when payment status is `"initiated"`.
    #[serde(default)]
    pub transaction_url: Option<String>,
    /// Issuer authorization approval code.
    #[serde(default)]
    pub authorization_code: Option<String>,
    /// Two-digit ISO 8583 acquirer response code.
    #[serde(default, serialize_with = "serialize_opt_response_code")]
    pub response_code: Option<ResponseCode>,
    /// Issuing bank name.
    #[serde(default)]
    pub issuer_name: Option<String>,
    /// Issuing bank ISO country code.
    #[serde(default)]
    pub issuer_country: Option<String>,
    /// Issuer card funding type (`debit`, `credit`, etc.).
    #[serde(default)]
    pub issuer_card_type: Option<IssuerCardType>,
    /// Issuer card product category (e.g., `"CLASSIC"`, `"PLATINUM"`).
    #[serde(default)]
    pub issuer_card_category: Option<String>,
}

/// Digital wallet (Apple Pay / Samsung Pay) source metadata returned by Moyasar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletSourceResponse {
    /// Underlying card network (`mada`, `visa`, `master`, etc.).
    #[serde(default)]
    pub company: Option<CardBrand>,
    /// Cardholder name if provided by the wallet.
    #[serde(default)]
    pub name: Option<String>,
    /// Masked PAN.
    #[serde(default)]
    pub number: Option<String>,
    /// Acquirer gateway transaction identifier.
    #[serde(default)]
    pub gateway_id: Option<String>,
    /// Bank reference number (RRN).
    #[serde(default)]
    pub reference_number: Option<String>,
    /// Reusable saved card token (`token_...`) when `save_card: true`.
    #[serde(default)]
    pub token: Option<String>,
    /// Human-readable gateway status message.
    #[serde(default)]
    pub message: Option<String>,
    /// Optional 3DS / redirect URL.
    #[serde(default)]
    pub transaction_url: Option<String>,
    /// Two-digit ISO 8583 acquirer response code.
    #[serde(default, serialize_with = "serialize_opt_response_code")]
    pub response_code: Option<ResponseCode>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::payments::source::{PaymentSourceRequest, PaymentSourceResponse};
    use crate::resources::payments::source_card::CreditCardSource;
    use crate::resources::payments::source_digital_wallet::{ApplePaySource, SamsungPaySource};
    use crate::resources::payments::source_token::TokenSource;

    #[test]
    fn tc_2_3_0_01_tagged_union_serialization_for_all_four_variants() {
        let card = CreditCardSource::new("Maher Mansour", "4111111111111111", 12, 2030, "123")
            .expect("valid card");
        let card_json = serde_json::to_value(PaymentSourceRequest::CreditCard(card)).expect("ok");
        assert_eq!(card_json["type"], "creditcard");

        let apple_json =
            serde_json::to_value(PaymentSourceRequest::ApplePay(ApplePaySource::new("ap_tok")))
                .expect("ok");
        assert_eq!(apple_json["type"], "applepay");

        let samsung_json = serde_json::to_value(PaymentSourceRequest::SamsungPay(
            SamsungPaySource::new("sp_tok"),
        ))
        .expect("ok");
        assert_eq!(samsung_json["type"], "samsungpay");

        let token = TokenSource::new("token_abc").expect("valid token");
        let token_json = serde_json::to_value(PaymentSourceRequest::Token(token)).expect("ok");
        assert_eq!(token_json["type"], "token");
    }

    #[test]
    fn tc_2_3_0_07_deserializes_and_roundtrips_full_issuer_metadata() {
        let raw = r#"{
            "type": "creditcard",
            "company": "mada",
            "name": "Maher Mansour",
            "number": "4111-XXXX-XXXX-1111",
            "gateway_id": "moyasar_gw_123",
            "reference_number": "9876543210",
            "token": "token_saved_001",
            "message": "APPROVED",
            "transaction_url": "https://api.moyasar.com/v1/payments/3ds/123",
            "authorization_code": "123456",
            "response_code": "00",
            "issuer_name": "Al Rajhi Bank",
            "issuer_country": "SA",
            "issuer_card_type": "debit",
            "issuer_card_category": "CLASSIC"
        }"#;
        let parsed: PaymentSourceResponse = serde_json::from_str(raw).expect("deserializes");
        match &parsed {
            PaymentSourceResponse::CreditCard(card) => {
                assert_eq!(card.company, Some(CardBrand::Mada));
                assert_eq!(card.response_code, Some(ResponseCode::IssuerApproved));
                assert_eq!(card.issuer_card_type, Some(IssuerCardType::Debit));
                assert_eq!(card.token.as_deref(), Some("token_saved_001"));
            }
            other => panic!("expected CreditCard response, got {other:?}"),
        }
        let roundtrip_json = serde_json::to_string(&parsed).expect("serializes");
        let reparsed: PaymentSourceResponse =
            serde_json::from_str(&roundtrip_json).expect("re-deserializes");
        assert_eq!(parsed, reparsed);
    }
}
