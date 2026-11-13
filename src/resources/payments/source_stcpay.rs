//! STC Pay payment source and Saudi mobile normalization (US-2.4.0 Unit 1).

use serde::{Deserialize, Serialize};

use crate::kernel::error::MoyasarError;

/// Canonical 10-digit Saudi mobile prefix (`05`).
const SAUDI_MOBILE_PREFIX: &str = "05";

/// Number of subscriber digits following the `05` prefix.
const SAUDI_SUBSCRIBER_LEN: usize = 8;

/// STC Pay payment source request payload.
///
/// Normalizes Saudi mobile numbers from `05xxxxxxxx`, `+9665xxxxxxxx`, or
/// `009665xxxxxxxx` into the canonical `05xxxxxxxx` format expected by Moyasar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StcPaySource {
    /// Customer Saudi mobile number normalized to `05xxxxxxxx`.
    pub mobile: String,
    /// Optional cashier identifier for point-of-sale or branch tracking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cashier: Option<String>,
    /// Optional merchant branch identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

/// STC Pay payment source returned in an initiated or completed payment response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StcPaySourceResponse {
    /// Source type discriminator (`"stcpay"`).
    #[serde(rename = "type", default)]
    pub source_type: Option<String>,
    /// Customer mobile number associated with the STC Pay charge.
    #[serde(default)]
    pub mobile: Option<String>,
    /// STC Pay reference number assigned to the transaction.
    #[serde(default)]
    pub reference_number: Option<String>,
    /// Optional merchant branch identifier.
    #[serde(default)]
    pub branch: Option<String>,
    /// Optional cashier identifier.
    #[serde(default)]
    pub cashier: Option<String>,
    /// Server-provided OTP verification URL (`/v1/stc_pays/:id/proceed?otp_token=...`).
    #[serde(default)]
    pub transaction_url: Option<String>,
    /// Human-readable gateway status message.
    #[serde(default)]
    pub message: Option<String>,
}

/// Validates and normalizes a Saudi mobile number to canonical `05xxxxxxxx` form.
///
/// Accepted input formats (after trimming surrounding whitespace):
/// - `05xxxxxxxx` (10 digits)
/// - `+9665xxxxxxxx` (13 characters)
/// - `009665xxxxxxxx` (14 digits)
///
/// # Errors
///
/// Returns [`MoyasarError::InvalidMobile`] when the prefix is unrecognized or
/// the subscriber suffix is not exactly 8 ASCII digits.
pub fn normalize_saudi_mobile(input: &str) -> Result<String, MoyasarError> {
    let trimmed = input.trim();
    let subscriber = if let Some(rest) = trimmed.strip_prefix("009665") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("+9665") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("05") {
        rest
    } else {
        return Err(MoyasarError::InvalidMobile);
    };

    if subscriber.len() != SAUDI_SUBSCRIBER_LEN || !subscriber.bytes().all(|b| b.is_ascii_digit()) {
        return Err(MoyasarError::InvalidMobile);
    }

    let mut normalized = String::with_capacity(SAUDI_MOBILE_PREFIX.len() + SAUDI_SUBSCRIBER_LEN);
    normalized.push_str(SAUDI_MOBILE_PREFIX);
    normalized.push_str(subscriber);
    Ok(normalized)
}

impl StcPaySource {
    /// Constructs a validated [`StcPaySource`] from a Saudi mobile number.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::InvalidMobile`] if `mobile` does not match
    /// `05xxxxxxxx`, `+9665xxxxxxxx`, or `009665xxxxxxxx`.
    pub fn new(mobile: &str) -> Result<Self, MoyasarError> {
        let normalized = normalize_saudi_mobile(mobile)?;
        Ok(Self {
            mobile: normalized,
            cashier: None,
            branch: None,
        })
    }

    /// Attaches an optional cashier identifier to the STC Pay source.
    #[must_use]
    pub fn with_cashier(mut self, cashier: impl Into<String>) -> Self {
        self.cashier = Some(cashier.into());
        self
    }

    /// Attaches an optional branch identifier to the STC Pay source.
    #[must_use]
    pub fn with_branch(mut self, branch: impl Into<String>) -> Self {
        self.branch = Some(branch.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_2_4_0_01_normalizes_all_three_saudi_mobile_formats() {
        let local = StcPaySource::new("0512345678").expect("05 format valid");
        let plus = StcPaySource::new("+966512345678").expect("+966 format valid");
        let intl = StcPaySource::new("00966512345678").expect("00966 format valid");

        assert_eq!(local.mobile, "0512345678");
        assert_eq!(plus.mobile, "0512345678");
        assert_eq!(intl.mobile, "0512345678");
        assert_eq!(local, plus);
        assert_eq!(plus, intl);
    }

    #[test]
    fn tc_2_4_0_01_rejects_invalid_saudi_mobile_inputs() {
        let invalid_inputs = [
            "12345",
            "",
            "0612345678",
            "+966412345678",
            "0096651234567",
            "05123456789",
            "051234567a",
        ];

        for raw in invalid_inputs {
            let err = StcPaySource::new(raw).expect_err("must reject invalid mobile");
            assert!(matches!(err, MoyasarError::InvalidMobile));
        }
    }

    #[test]
    fn builds_with_optional_cashier_and_branch_and_serializes_cleanly() {
        let src = StcPaySource::new("  +966515555555  ")
            .expect("trimmed +966 valid")
            .with_cashier("POS-01")
            .with_branch("RIYADH-MAIN");

        let json = serde_json::to_value(&src).expect("serializes");
        assert_eq!(json["mobile"], "0515555555");
        assert_eq!(json["cashier"], "POS-01");
        assert_eq!(json["branch"], "RIYADH-MAIN");

        let minimal = StcPaySource::new("0515555555").expect("valid");
        let minimal_json = serde_json::to_value(&minimal).expect("serializes");
        assert!(minimal_json.get("cashier").is_none());
        assert!(minimal_json.get("branch").is_none());
    }
}
