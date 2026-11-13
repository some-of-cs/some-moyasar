//! Credit and debit card payment source with PCI-DSS 4.0 redaction (US-2.3.0 Unit 2).

use core::fmt;

use secrecy::{ExposeSecret, SecretString};
use serde::{Serialize, Serializer};

use crate::kernel::error::MoyasarError;

use super::card_validation::validate_card_fields;

fn serialize_secret<S: Serializer>(
    secret: &SecretString,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(secret.expose_secret())
}

/// Credit or debit card payment source (`type: "creditcard"`).
///
/// Sensitive fields (`number` and `cvc`) are stored in [`SecretString`] (zeroized on drop)
/// and are always masked as `"[REDACTED]"` in [`fmt::Debug`] output.
#[derive(Clone, Serialize)]
pub struct CreditCardSource {
    /// Cardholder name as printed on the card.
    pub name: String,
    /// Primary Account Number (PAN), zeroized on drop and never logged.
    #[serde(serialize_with = "serialize_secret")]
    pub number: SecretString,
    /// Two-digit expiration month (`1..=12`).
    pub month: u8,
    /// Expiration year (2-digit or 4-digit).
    pub year: u16,
    /// Card Verification Code (3 digits, or 4 digits for AMEX), zeroized on drop.
    #[serde(serialize_with = "serialize_secret")]
    pub cvc: SecretString,
    /// Optional statement descriptor shown on the cardholder's bank statement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statement_descriptor: Option<String>,
    /// When `true`, authorizes without immediate capture (manual capture).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manual: Option<bool>,
    /// When `true`, tokenizes the card and returns `source.token` before 3DS redirect.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save_card: Option<bool>,
    /// Explicit 3-D Secure challenge toggle (`"3ds"` on the wire).
    #[serde(rename = "3ds", skip_serializing_if = "Option::is_none")]
    pub three_ds: Option<bool>,
    /// Optional pre-authenticated card auth ID (`ca_...`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_auth_id: Option<String>,
    /// Optional external 3DS authentication payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_auth_data: Option<serde_json::Value>,
}

impl CreditCardSource {
    /// Constructs and validates a new [`CreditCardSource`] before any network call.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::InvalidCardNumber`], [`MoyasarError::CardExpired`], or
    /// [`MoyasarError::InvalidCvc`] if client-side validation fails.
    pub fn new(
        name: impl Into<String>,
        number: impl Into<String>,
        month: u8,
        year: u16,
        cvc: impl Into<String>,
    ) -> Result<Self, MoyasarError> {
        let number_str = number.into();
        let cvc_str = cvc.into();
        validate_card_fields(&number_str, month, year, &cvc_str)?;
        Ok(Self {
            name: name.into(),
            number: SecretString::from(number_str),
            month,
            year,
            cvc: SecretString::from(cvc_str),
            statement_descriptor: None,
            manual: None,
            save_card: None,
            three_ds: None,
            card_auth_id: None,
            card_auth_data: None,
        })
    }

    /// Validates card number (Luhn), expiry date, CVC length, and optional `card_auth_id` prefix.
    pub fn validate(&self) -> Result<(), MoyasarError> {
        validate_card_fields(
            self.number.expose_secret(),
            self.month,
            self.year,
            self.cvc.expose_secret(),
        )?;
        if let Some(auth_id) = &self.card_auth_id
            && !auth_id.starts_with("ca_")
        {
            return Err(MoyasarError::InvalidCardAuthId);
        }
        Ok(())
    }
}

impl fmt::Debug for CreditCardSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreditCardSource")
            .field("name", &self.name)
            .field("number", &"[REDACTED]")
            .field("month", &self.month)
            .field("year", &self.year)
            .field("cvc", &"[REDACTED]")
            .field("statement_descriptor", &self.statement_descriptor)
            .field("manual", &self.manual)
            .field("save_card", &self.save_card)
            .field("three_ds", &self.three_ds)
            .field("card_auth_id", &self.card_auth_id)
            .field("card_auth_data", &self.card_auth_data)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_2_3_0_law_credit_card_debug_redacts_pan_and_cvc() {
        let card = CreditCardSource::new("Maher Mansour", "4111111111111111", 12, 2030, "123")
            .expect("valid card");
        let debug_str = format!("{card:?}");
        assert!(debug_str.contains("[REDACTED]"));
        assert!(!debug_str.contains("4111111111111111"));
        assert!(!debug_str.contains("123"));
    }

    #[test]
    fn serializes_three_ds_flag_with_wire_name() {
        let mut card = CreditCardSource::new("Maher Mansour", "4111111111111111", 12, 2030, "123")
            .expect("valid card");
        card.three_ds = Some(true);
        card.save_card = Some(true);
        let json = serde_json::to_value(&card).expect("serializes");
        assert_eq!(json["3ds"], true);
        assert_eq!(json["save_card"], true);
        assert!(json.get("manual").is_none());
    }
}
