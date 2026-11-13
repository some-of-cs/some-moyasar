//! Payment Intent fulfillment request parameters (`FulfillIntentParams`).

use core::fmt;

use secrecy::{ExposeSecret, SecretString};
use serde::{Serialize, Serializer};
use zeroize::Zeroize;

use crate::kernel::error::MoyasarError;
use crate::kernel::metadata::Metadata;
use crate::resources::payment_intents::fulfill_guard::validate_client_secret;

/// Payment instrument payload accepted by `POST /v1/payment_intents/:id/fulfill`.
///
/// Sensitive card numbers, CVCs, tokens, and mobile numbers are zeroized in memory on drop.
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PaymentSourceRequest {
    /// Raw credit or debit card details collected in the browser/mobile SDK.
    CreditCard {
        /// Cardholder name.
        name: String,
        /// Primary account number (PAN).
        number: String,
        /// Two-digit expiry month (`MM`).
        month: String,
        /// Two- or four-digit expiry year (`YY` / `YYYY`).
        year: String,
        /// Card verification code.
        cvc: String,
    },
    /// Previously tokenized payment instrument (`token_...`).
    Token {
        /// Moyasar token identifier.
        token: String,
    },
    /// Apple Pay encrypted payment token payload.
    ApplePay {
        /// Serialized Apple Pay token.
        token: String,
    },
    /// STC Pay mobile wallet identifier.
    #[serde(rename = "stcpay")]
    StcPay {
        /// Saudi mobile number (`05XXXXXXXX`).
        mobile: String,
    },
}

impl Drop for PaymentSourceRequest {
    fn drop(&mut self) {
        match self {
            Self::CreditCard {
                name,
                number,
                month,
                year,
                cvc,
            } => {
                name.zeroize();
                number.zeroize();
                month.zeroize();
                year.zeroize();
                cvc.zeroize();
            }
            Self::Token { token } | Self::ApplePay { token } => {
                token.zeroize();
            }
            Self::StcPay { mobile } => {
                mobile.zeroize();
            }
        }
    }
}

impl fmt::Debug for PaymentSourceRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreditCard { name, .. } => f
                .debug_struct("CreditCard")
                .field("name", name)
                .field("number", &"[REDACTED]")
                .field("cvc", &"[REDACTED]")
                .finish(),
            Self::Token { .. } => f.write_str("Token([REDACTED])"),
            Self::ApplePay { .. } => f.write_str("ApplePay([REDACTED])"),
            Self::StcPay { mobile } => f.debug_struct("StcPay").field("mobile", mobile).finish(),
        }
    }
}

fn serialize_secret<S: Serializer>(
    secret: &SecretString,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(secret.expose_secret())
}

/// Request payload for fulfilling a [`PaymentIntent`](crate::resources::payment_intents::models::PaymentIntent)
/// from a browser or mobile client using a `PublishableKey` and `client_secret`.
///
/// Locked server-side fields (`amount`, `currency`, `description`, `callback_url`) are
/// intentionally omitted so client overrides cannot be transmitted.
#[derive(Clone, Serialize)]
pub struct FulfillIntentParams {
    /// Ephemeral client secret issued when the intent was created.
    #[serde(serialize_with = "serialize_secret")]
    client_secret: SecretString,
    /// Payment instrument source details.
    pub source: PaymentSourceRequest,
    /// Optional metadata attached to the resulting payment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    /// Whether to save the card token for future off-session charges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save_card: Option<bool>,
    /// Whether to authorize only (`true`) without immediate capture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manual: Option<bool>,
    /// Whether to enforce 3-D Secure authentication (`"3ds"` in JSON).
    #[serde(rename = "3ds", skip_serializing_if = "Option::is_none")]
    pub three_ds: Option<bool>,
}

impl FulfillIntentParams {
    /// Constructs a [`FulfillIntentParams`] payload, rejecting empty or whitespace-padded
    /// `client_secret` values.
    ///
    /// # Examples
    ///
    /// ```
    /// use some_moyasar::resources::payment_intents::fulfill_params::{
    ///     FulfillIntentParams, PaymentSourceRequest,
    /// };
    ///
    /// let params = FulfillIntentParams::new(
    ///     "cs_test_secret_123",
    ///     PaymentSourceRequest::Token { token: "token_abc".to_owned() },
    /// )
    /// .unwrap()
    /// .save_card(true)
    /// .three_ds(true);
    /// assert_eq!(params.client_secret(), "cs_test_secret_123");
    /// ```
    pub fn new(
        client_secret: impl Into<String>,
        source: PaymentSourceRequest,
    ) -> Result<Self, MoyasarError> {
        let raw = client_secret.into();
        validate_client_secret(&raw)?;
        Ok(Self {
            client_secret: SecretString::from(raw),
            source,
            metadata: None,
            save_card: None,
            manual: None,
            three_ds: None,
        })
    }

    /// Exposes the underlying `client_secret` slice for request signing or inspection.
    pub fn client_secret(&self) -> &str {
        self.client_secret.expose_secret()
    }

    /// Sets optional [`Metadata`] on the fulfillment request.
    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Sets the `save_card` flag.
    pub fn save_card(mut self, save_card: bool) -> Self {
        self.save_card = Some(save_card);
        self
    }

    /// Sets the `manual` authorization-only flag.
    pub fn manual(mut self, manual: bool) -> Self {
        self.manual = Some(manual);
        self
    }

    /// Sets the `3ds` enforcement flag.
    pub fn three_ds(mut self, three_ds: bool) -> Self {
        self.three_ds = Some(three_ds);
        self
    }
}

impl fmt::Debug for FulfillIntentParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FulfillIntentParams")
            .field("client_secret", &"[REDACTED]")
            .field("source", &self.source)
            .field("metadata", &self.metadata)
            .field("save_card", &self.save_card)
            .field("manual", &self.manual)
            .field("three_ds", &self.three_ds)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_3_2_0_06_serializes_save_card_and_3ds_without_locked_fields() {
        let params = FulfillIntentParams::new(
            "cs_secret_xyz",
            PaymentSourceRequest::Token {
                token: "token_123".to_owned(),
            },
        )
        .expect("valid")
        .save_card(true)
        .three_ds(true);

        let json = serde_json::to_value(&params).expect("serializes");
        assert_eq!(json["client_secret"], "cs_secret_xyz");
        assert_eq!(json["save_card"], true);
        assert_eq!(json["3ds"], true);
        assert!(json.get("amount").is_none());
        assert!(json.get("currency").is_none());
        assert!(json.get("description").is_none());
        assert!(json.get("callback_url").is_none());
    }

    #[test]
    fn redacts_client_secret_in_debug_and_rejects_empty_or_padded_secret() {
        for bad in ["", "  ", " cs_padded", "cs_padded "] {
            assert!(
                FulfillIntentParams::new(
                    bad,
                    PaymentSourceRequest::Token {
                        token: "token_123".to_owned(),
                    },
                )
                .is_err(),
                "expected Err for {bad:?}"
            );
        }

        let params = FulfillIntentParams::new(
            "cs_do_not_leak_in_logs",
            PaymentSourceRequest::Token {
                token: "token_123".to_owned(),
            },
        )
        .expect("valid");
        let dbg = format!("{params:?}");
        assert!(dbg.contains("[REDACTED]"));
        assert!(!dbg.contains("cs_do_not_leak_in_logs"));
    }
}
