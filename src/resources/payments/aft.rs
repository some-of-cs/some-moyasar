//! Account Funding Transaction (`AftSender` / `AfTSender`) types (US-2.5.0 Unit 3).

use core::fmt;

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

/// Sender identification payload for Account Funding Transactions (AFT).
///
/// The `id_number` and `iban` fields are redacted as `"[REDACTED]"` in [`fmt::Debug`]
/// output so government identification numbers and IBANs never leak into application logs,
/// and all sender fields are zeroized in memory on drop.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AftSender {
    /// Sender's legal first name.
    pub first_name: String,
    /// Sender's legal last name.
    pub last_name: String,
    /// Optional ISO 3166-1 alpha-2/alpha-3 nationality code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nationality: Option<String>,
    /// Optional government identification document type (e.g., `"national_id"`, `"passport"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_type: Option<String>,
    /// Optional government identification number (redacted in `Debug` output).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_number: Option<String>,
    /// Optional sender IBAN (redacted in `Debug` output).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iban: Option<String>,
    /// Optional sender residential address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
}

fn zeroize_opt_string(opt: &mut Option<String>) {
    if let Some(value) = opt {
        value.zeroize();
    }
}

impl Zeroize for AftSender {
    fn zeroize(&mut self) {
        self.first_name.zeroize();
        self.last_name.zeroize();
        zeroize_opt_string(&mut self.nationality);
        zeroize_opt_string(&mut self.id_type);
        zeroize_opt_string(&mut self.id_number);
        zeroize_opt_string(&mut self.iban);
        zeroize_opt_string(&mut self.address);
    }
}

impl Drop for AftSender {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl fmt::Debug for AftSender {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AftSender")
            .field("first_name", &self.first_name)
            .field("last_name", &self.last_name)
            .field("nationality", &self.nationality)
            .field("id_type", &self.id_type)
            .field("id_number", &self.id_number.as_ref().map(|_| "[REDACTED]"))
            .field("iban", &self.iban.as_ref().map(|_| "[REDACTED]"))
            .field("address", &self.address)
            .finish()
    }
}

/// Compatibility type alias for [`AftSender`].
pub type AfTSender = AftSender;

impl AftSender {
    /// Constructs a new [`AftSender`] with required `first_name` and `last_name`.
    pub fn new(first_name: impl Into<String>, last_name: impl Into<String>) -> Self {
        Self {
            first_name: first_name.into(),
            last_name: last_name.into(),
            nationality: None,
            id_type: None,
            id_number: None,
            iban: None,
            address: None,
        }
    }

    /// Sets the sender's `nationality`.
    pub fn with_nationality(mut self, nationality: impl Into<String>) -> Self {
        self.nationality = Some(nationality.into());
        self
    }

    /// Sets the sender's `id_type` and `id_number`.
    pub fn with_id(mut self, id_type: impl Into<String>, id_number: impl Into<String>) -> Self {
        self.id_type = Some(id_type.into());
        self.id_number = Some(id_number.into());
        self
    }

    /// Sets the sender's `iban`.
    #[must_use]
    pub fn with_iban(mut self, iban: impl Into<String>) -> Self {
        self.iban = Some(iban.into());
        self
    }

    /// Sets the sender's `address`.
    #[must_use]
    pub fn with_address(mut self, address: impl Into<String>) -> Self {
        self.address = Some(address.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tc_2_5_0_06_serializes_aft_sender_and_alias() {
        let sender: AfTSender = AftSender::new("Maher", "Mansour")
            .with_nationality("SA")
            .with_id("national_id", "1020304050");

        let val = serde_json::to_value(&sender).expect("AftSender must serialize");
        assert_eq!(val["first_name"], json!("Maher"));
        assert_eq!(val["last_name"], json!("Mansour"));
        assert_eq!(val["nationality"], json!("SA"));
        assert_eq!(val["id_type"], json!("national_id"));
        assert_eq!(val["id_number"], json!("1020304050"));

        let minimal = AftSender::new("Sara", "Ali");
        let min_val = serde_json::to_value(&minimal).expect("minimal AftSender must serialize");
        assert!(min_val.get("nationality").is_none());
        assert!(min_val.get("id_type").is_none());
        assert!(min_val.get("id_number").is_none());
    }

    #[test]
    fn aft_sender_debug_redacts_id_number_and_iban() {
        let sender = AftSender::new("Maher", "Mansour")
            .with_nationality("SA")
            .with_id("national_id", "1020304050")
            .with_iban("SA0380000000608010167519")
            .with_address("Riyadh");
        let dbg = format!("{sender:?}");
        assert!(dbg.contains("[REDACTED]"));
        assert!(!dbg.contains("1020304050"));
        assert!(!dbg.contains("SA0380000000608010167519"));
    }
}
