//! SADAD bill payment source and response types (US-2.4.0 Unit 3).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Primary production SADAD biller code.
pub const SADAD_BILLER_PRODUCTION_PRIMARY: &str = "901";

/// Secondary production SADAD biller code.
pub const SADAD_BILLER_PRODUCTION_SECONDARY: &str = "902";

/// Sandbox / test-mode SADAD biller code.
pub const SADAD_BILLER_SANDBOX: &str = "007";

/// SADAD bill payment source request payload.
///
/// Generates a payable `sadad_number` settled asynchronously by the customer
/// through their banking app or ATM.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SadadBillSource {
    /// Optional bill expiration timestamp (defaults to 29 days; capped at 29 days).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expired_at: Option<DateTime<Utc>>,
    /// Optional SADAD biller code (e.g., `"007"` for sandbox, `"901"` or `"902"` for production).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub biller_code: Option<String>,
}

/// SADAD bill payment source returned by the Moyasar API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SadadBillSourceResponse {
    /// Human-readable provider label (e.g., sandbox provider name).
    #[serde(default)]
    pub provider_name: Option<String>,
    /// Assigned SADAD biller code (`"901"`, `"902"`, or `"007"`).
    pub biller_code: String,
    /// Numeric SADAD bill reference payable via bank or ATM.
    pub sadad_number: String,
    /// Optional gateway transaction identifier.
    #[serde(default)]
    pub gateway_id: Option<String>,
    /// Bill expiration timestamp in UTC.
    #[serde(default)]
    pub expired_at: Option<DateTime<Utc>>,
    /// Optional gateway status message.
    #[serde(default)]
    pub message: Option<String>,
}

impl SadadBillSource {
    /// Creates a default [`SadadBillSource`] with server-assigned expiry and biller code.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            expired_at: None,
            biller_code: None,
        }
    }

    /// Sets an explicit UTC expiration timestamp on the SADAD bill.
    #[must_use]
    pub const fn with_expired_at(mut self, expired_at: DateTime<Utc>) -> Self {
        self.expired_at = Some(expired_at);
        self
    }

    /// Sets an explicit SADAD biller code (such as `"007"` in sandbox mode).
    #[must_use]
    pub fn with_biller_code(mut self, biller_code: impl Into<String>) -> Self {
        self.biller_code = Some(biller_code.into());
        self
    }

    /// Returns `true` if `biller_code` is one of the known Moyasar SADAD codes (`901`, `902`, `007`).
    #[must_use]
    pub fn is_known_biller_code(biller_code: &str) -> bool {
        matches!(
            biller_code,
            SADAD_BILLER_PRODUCTION_PRIMARY
                | SADAD_BILLER_PRODUCTION_SECONDARY
                | SADAD_BILLER_SANDBOX
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_2_4_0_04_and_06_serializes_and_deserializes_sadad_biller_codes() {
        let default_req = SadadBillSource::new();
        let default_json = serde_json::to_value(&default_req).expect("serializes");
        assert!(default_json.get("expired_at").is_none());
        assert!(default_json.get("biller_code").is_none());

        let sandbox_req = SadadBillSource::new().with_biller_code(SADAD_BILLER_SANDBOX);
        let sandbox_json = serde_json::to_value(&sandbox_req).expect("serializes");
        assert_eq!(sandbox_json["biller_code"], "007");

        let raw_response = r#"{
            "provider_name": "Moyasar Sandbox SADAD",
            "biller_code": "007",
            "sadad_number": "900012345678",
            "gateway_id": "sadad_gw_01",
            "expired_at": "2026-12-12T12:00:00Z",
            "message": "Bill created"
        }"#;
        let parsed: SadadBillSourceResponse =
            serde_json::from_str(raw_response).expect("deserializes SADAD response");
        assert_eq!(parsed.biller_code, "007");
        assert_eq!(parsed.sadad_number, "900012345678");
        assert!(parsed.sadad_number.bytes().all(|b| b.is_ascii_digit()));
        assert_eq!(
            parsed.provider_name.as_deref(),
            Some("Moyasar Sandbox SADAD")
        );
        assert!(SadadBillSource::is_known_biller_code(&parsed.biller_code));
        assert!(SadadBillSource::is_known_biller_code("901"));
        assert!(SadadBillSource::is_known_biller_code("902"));
        assert!(!SadadBillSource::is_known_biller_code("999"));
    }
}
