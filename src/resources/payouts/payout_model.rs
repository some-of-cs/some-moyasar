//! Payout disbursement response models (`US-5.2.0` Unit 1).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

use crate::kernel::error::{ApiErrorType, MoyasarError};

/// Strongly-typed identifier for a Moyasar payout disbursement.
///
/// Rejects empty or whitespace-only identifiers on construction and
/// deserialization.
///
/// # Examples
///
/// ```
/// use std::str::FromStr;
/// use some_moyasar::resources::payouts::payout_model::PayoutId;
///
/// let id = PayoutId::new("pay_123456").expect("valid payout id");
/// assert_eq!(id.as_str(), "pay_123456");
/// assert_eq!(id.to_string(), "pay_123456");
///
/// let parsed = PayoutId::from_str("pay_123456").expect("valid payout id");
/// assert_eq!(id, parsed);
///
/// assert!(PayoutId::new("   ").is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct PayoutId(String);

impl PayoutId {
    /// Creates a new [`PayoutId`], returning an error if `id` is empty or
    /// contains only whitespace.
    pub fn new(id: impl Into<String>) -> Result<Self, MoyasarError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                message: "payout id must not be empty".into(),
                errors: vec![],
                response_code: None,
            });
        }
        Ok(Self(id))
    }

    /// Returns the underlying string slice for this payout identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PayoutId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for PayoutId {
    type Err = MoyasarError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl<'de> Deserialize<'de> for PayoutId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::new(raw).map_err(serde::de::Error::custom)
    }
}

/// Lifecycle status of a Moyasar payout disbursement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PayoutStatus {
    /// Payout has been accepted and is queued for processing.
    Queued,
    /// Payout has been initiated with the transfer rail.
    Initiated,
    /// Payout has been completed and settled.
    Paid,
    /// Payout failed during processing.
    Failed,
    /// Payout was canceled before settlement.
    Canceled,
    /// Payout was returned by the receiving institution.
    Returned,
}

/// Transfer rail channel used to execute a Moyasar payout disbursement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PayoutChannel {
    /// Internal Moyasar ledger transfer.
    Internal,
    /// Instant Payment System (IPS) rail.
    Ips,
    /// Saudi Arabian Riyal Interbank Express (SARIE) rail.
    Sarie,
    /// Forward-compatible fallback for unrecognized payout channels.
    #[serde(other)]
    Unknown,
}

/// Moyasar payout disbursement resource returned by the Payouts API.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::payouts::payout_model::{
///     Payout, PayoutChannel, PayoutId, PayoutStatus,
/// };
///
/// let json = r#"{
///     "id": "pout_abc123",
///     "channel": "sarie",
///     "status": "paid",
///     "amount": 50000,
///     "message": "Vendor settlement",
///     "failure_reason": null
/// }"#;
///
/// let payout: Payout = serde_json::from_str(json).expect("valid payout JSON");
/// assert_eq!(payout.id, PayoutId::new("pout_abc123").unwrap());
/// assert_eq!(payout.channel, PayoutChannel::Sarie);
/// assert_eq!(payout.status, PayoutStatus::Paid);
/// assert_eq!(payout.amount, 50_000);
/// assert_eq!(payout.message.as_deref(), Some("Vendor settlement"));
/// assert_eq!(payout.failure_reason, None);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payout {
    /// Unique identifier of the payout.
    pub id: PayoutId,
    /// Transfer channel used to disburse the payout.
    pub channel: PayoutChannel,
    /// Current lifecycle status of the payout.
    pub status: PayoutStatus,
    /// Disbursement amount in the smallest currency unit (halalas).
    pub amount: u64,
    /// Optional human-readable description or status message.
    pub message: Option<String>,
    /// Optional reason describing why a payout failed or was returned.
    pub failure_reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payout_id_accepts_valid_and_rejects_empty_or_whitespace() {
        let valid = PayoutId::new("pout_01JXYZ").expect("non-empty id should succeed");
        assert_eq!(valid.as_str(), "pout_01JXYZ");
        assert_eq!(valid.to_string(), "pout_01JXYZ");

        let parsed: PayoutId = "pout_01JXYZ".parse().expect("FromStr should succeed");
        assert_eq!(parsed, valid);

        for invalid in ["", "   ", "\t\n"] {
            let err = PayoutId::new(invalid).expect_err("empty/whitespace id must fail");
            match err {
                MoyasarError::Api {
                    error_type,
                    message,
                    errors,
                    response_code,
                } => {
                    assert_eq!(error_type, ApiErrorType::InvalidRequestError);
                    assert_eq!(message, "payout id must not be empty");
                    assert!(errors.is_empty());
                    assert_eq!(response_code, None);
                }
                other => panic!("expected MoyasarError::Api, got {other:?}"),
            }

            assert!(invalid.parse::<PayoutId>().is_err());
        }
    }

    #[test]
    fn payout_id_serde_roundtrip_and_validation() {
        let id = PayoutId::new("pout_999").unwrap();
        let serialized = serde_json::to_string(&id).unwrap();
        assert_eq!(serialized, r#""pout_999""#);

        let deserialized: PayoutId = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, id);

        assert!(serde_json::from_str::<PayoutId>(r#""""#).is_err());
        assert!(serde_json::from_str::<PayoutId>(r#""   ""#).is_err());
    }

    #[test]
    fn payout_status_deserializes_and_serializes_all_six_variants() {
        let cases = [
            ("queued", PayoutStatus::Queued),
            ("initiated", PayoutStatus::Initiated),
            ("paid", PayoutStatus::Paid),
            ("failed", PayoutStatus::Failed),
            ("canceled", PayoutStatus::Canceled),
            ("returned", PayoutStatus::Returned),
        ];

        for (wire, expected) in cases {
            let json = format!(r#""{wire}""#);
            let parsed: PayoutStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, expected);

            let encoded = serde_json::to_string(&parsed).unwrap();
            assert_eq!(encoded, json);
        }
    }

    #[test]
    fn payout_channel_deserializes_known_variants_and_unknown_fallback() {
        let known = [
            ("internal", PayoutChannel::Internal),
            ("ips", PayoutChannel::Ips),
            ("sarie", PayoutChannel::Sarie),
        ];

        for (wire, expected) in known {
            let json = format!(r#""{wire}""#);
            let parsed: PayoutChannel = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, expected);

            let encoded = serde_json::to_string(&parsed).unwrap();
            assert_eq!(encoded, json);
        }

        let unknown: PayoutChannel = serde_json::from_str(r#""swift""#).unwrap();
        assert_eq!(unknown, PayoutChannel::Unknown);
    }

    #[test]
    fn payout_deserializes_with_optional_fields() {
        let json = r#"{
            "id": "pout_failed_1",
            "channel": "ips",
            "status": "failed",
            "amount": 12500,
            "message": "Transfer rejected",
            "failure_reason": "Invalid beneficiary IBAN"
        }"#;

        let payout: Payout = serde_json::from_str(json).unwrap();
        assert_eq!(payout.id.as_str(), "pout_failed_1");
        assert_eq!(payout.channel, PayoutChannel::Ips);
        assert_eq!(payout.status, PayoutStatus::Failed);
        assert_eq!(payout.amount, 12_500);
        assert_eq!(payout.message.as_deref(), Some("Transfer rejected"));
        assert_eq!(
            payout.failure_reason.as_deref(),
            Some("Invalid beneficiary IBAN")
        );

        let minimal_json = r#"{
            "id": "pout_queued_1",
            "channel": "future_rail",
            "status": "queued",
            "amount": 1000
        }"#;

        let minimal: Payout = serde_json::from_str(minimal_json).unwrap();
        assert_eq!(minimal.channel, PayoutChannel::Unknown);
        assert_eq!(minimal.status, PayoutStatus::Queued);
        assert_eq!(minimal.message, None);
        assert_eq!(minimal.failure_reason, None);
    }
}
