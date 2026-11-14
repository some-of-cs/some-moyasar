//! Payout account response models (`US-5.1.0` Unit 1).

use core::fmt;
use std::collections::BTreeMap;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::types::Currency;

/// Strongly typed, non-empty identifier for a Moyasar payout account.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::payouts::models::PayoutAccountId;
///
/// let id = PayoutAccountId::new("pa_01J9XYZ").expect("valid payout account id");
/// assert_eq!(id.as_str(), "pa_01J9XYZ");
/// assert_eq!(id.to_string(), "pa_01J9XYZ");
/// assert!(PayoutAccountId::new("   ").is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct PayoutAccountId(String);

pub(crate) fn has_unsafe_path_chars(raw: &str) -> bool {
    raw.contains("..")
        || raw
            .chars()
            .any(|c| c.is_whitespace() || "/?#\\".contains(c))
}

impl PayoutAccountId {
    /// Constructs a new [`PayoutAccountId`], rejecting empty strings, whitespace,
    /// and URL path/query delimiter characters.
    pub fn new(id: impl Into<String>) -> Result<Self, MoyasarError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                message: "payout account id must not be empty".into(),
                errors: vec![],
                response_code: None,
            });
        }
        if has_unsafe_path_chars(&id) {
            return Err(MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                message: "payout account id must not contain whitespace or URL delimiters".into(),
                errors: vec![],
                response_code: None,
            });
        }
        Ok(Self(id))
    }

    /// Returns the underlying payout account identifier string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PayoutAccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for PayoutAccountId {
    type Err = MoyasarError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl<'de> Deserialize<'de> for PayoutAccountId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::new(raw).map_err(serde::de::Error::custom)
    }
}

/// Classification of a Moyasar payout account destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PayoutAccountType {
    /// Traditional bank account destination (e.g., IBAN transfer).
    Bank,
    /// Digital wallet destination.
    Wallet,
}

/// Account-type-specific properties returned for a Moyasar payout account.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PayoutAccountProperties {
    /// International Bank Account Number, when applicable.
    #[serde(default)]
    pub iban: Option<String>,
    /// Account holder or beneficiary name.
    #[serde(default)]
    pub name: Option<String>,
    /// Receiving bank name, when applicable.
    #[serde(default)]
    pub bank_name: Option<String>,
    /// Forward-compatible map of additional provider-specific properties.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Moyasar Payout Account resource returned by `/v1/payout_accounts` endpoints.
///
/// By design, this model omits any `credentials` field so secret API or
/// account credentials are never stored or exposed in `Debug` output.
///
/// # Examples
///
/// ```
/// use some_moyasar::prelude::Currency;
/// use some_moyasar::resources::payouts::models::{PayoutAccount, PayoutAccountType};
///
/// let json = r#"{
///     "id": "pa_01J9XYZ",
///     "account_type": "bank",
///     "currency": "SAR",
///     "properties": {
///         "iban": "SA0380000000608010167519",
///         "name": "Acme Trading LLC",
///         "bank_name": "Al Rajhi Bank"
///     },
///     "created_at": "2026-11-14T00:00:00Z"
/// }"#;
///
/// let account: PayoutAccount = serde_json::from_str(json).expect("valid payout account");
/// assert_eq!(account.id.as_str(), "pa_01J9XYZ");
/// assert_eq!(account.account_type, PayoutAccountType::Bank);
/// assert_eq!(account.currency, Currency::SAR);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayoutAccount {
    /// Unique Moyasar payout account identifier.
    pub id: PayoutAccountId,
    /// Payout account type (`bank` or `wallet`).
    pub account_type: PayoutAccountType,
    /// ISO 4217 settlement currency for the payout account.
    pub currency: Currency,
    /// Account-type-specific properties and forward-compatible extra fields.
    #[serde(default)]
    pub properties: PayoutAccountProperties,
    /// Timestamp when the payout account was created.
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ac_us_5_1_0_01_deserializes_bank_payout_account_and_extra_properties() {
        let payload = json!({
            "id": "pa_01J9BANK",
            "account_type": "bank",
            "currency": "SAR",
            "properties": {
                "iban": "SA0380000000608010167519",
                "name": "Acme Trading LLC",
                "bank_name": "Al Rajhi Bank",
                "swift_code": "RJHISARI"
            },
            "created_at": "2026-11-14T00:00:00Z"
        });

        let account: PayoutAccount = serde_json::from_value(payload).expect("valid bank account");
        assert_eq!(account.id.as_str(), "pa_01J9BANK");
        assert_eq!(account.account_type, PayoutAccountType::Bank);
        assert_eq!(account.currency, Currency::SAR);
        assert_eq!(
            account.properties.iban.as_deref(),
            Some("SA0380000000608010167519")
        );
        assert_eq!(account.properties.name.as_deref(), Some("Acme Trading LLC"));
        assert_eq!(
            account.properties.bank_name.as_deref(),
            Some("Al Rajhi Bank")
        );
        assert_eq!(
            account.properties.extra.get("swift_code"),
            Some(&json!("RJHISARI"))
        );
    }

    #[test]
    fn ac_us_5_1_0_02_deserializes_wallet_payout_account_and_roundtrips() {
        let payload = json!({
            "id": "pa_01J9WALLET",
            "account_type": "wallet",
            "currency": "SAR",
            "properties": {
                "name": "Merchant Wallet",
                "wallet_provider": "stcpay"
            },
            "created_at": "2026-11-14T01:00:00Z"
        });

        let account: PayoutAccount = serde_json::from_value(payload).expect("valid wallet account");
        assert_eq!(account.account_type, PayoutAccountType::Wallet);
        assert_eq!(
            account.properties.extra.get("wallet_provider"),
            Some(&json!("stcpay"))
        );

        let serialized = serde_json::to_value(&account).expect("serializes");
        let roundtripped: PayoutAccount =
            serde_json::from_value(serialized).expect("deserializes roundtrip");
        assert_eq!(roundtripped, account);
    }

    #[test]
    fn ac_us_5_1_0_05_no_credentials_field_and_no_secrets_in_debug() {
        let payload = json!({
            "id": "pa_01J9SAFE",
            "account_type": "bank",
            "currency": "SAR",
            "credentials": {
                "api_secret": "sk_live_supersecret_value",
                "client_secret": "secret_token_123"
            },
            "properties": {
                "iban": "SA0380000000608010167519",
                "name": "Safe Account"
            },
            "created_at": "2026-11-14T02:00:00Z"
        });

        let account: PayoutAccount =
            serde_json::from_value(payload).expect("ignores unknown credentials field");
        let dbg = format!("{account:?}");
        assert!(!dbg.contains("credentials"));
        assert!(!dbg.contains("sk_live_supersecret_value"));
        assert!(!dbg.contains("secret_token_123"));

        let reserialized = serde_json::to_value(&account).expect("serializes");
        assert!(reserialized.get("credentials").is_none());
    }

    #[test]
    fn payout_account_id_validates_non_empty_and_traits() {
        let id = PayoutAccountId::new("pa_valid_123").expect("non-empty id");
        assert_eq!(id.as_str(), "pa_valid_123");
        assert_eq!(id.to_string(), "pa_valid_123");
        assert_eq!(
            "pa_valid_123".parse::<PayoutAccountId>().expect("FromStr"),
            id
        );

        for invalid in ["", "   ", "\t\n"] {
            let err = PayoutAccountId::new(invalid).expect_err("must reject empty/whitespace");
            match err {
                MoyasarError::Api {
                    error_type,
                    message,
                    errors,
                    response_code,
                } => {
                    assert_eq!(error_type, ApiErrorType::InvalidRequestError);
                    assert_eq!(message, "payout account id must not be empty");
                    assert!(errors.is_empty());
                    assert_eq!(response_code, None);
                }
                other => panic!("expected MoyasarError::Api, got {other:?}"),
            }

            assert!(invalid.parse::<PayoutAccountId>().is_err());

            let json_str = serde_json::to_string(invalid).expect("json string");
            assert!(serde_json::from_str::<PayoutAccountId>(&json_str).is_err());
        }
    }
}
