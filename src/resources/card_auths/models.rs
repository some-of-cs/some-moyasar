//! Standalone 3DS card authentication models (`CardAuthentication`, `CardAuthStatus`, `CardAuthId`, `CardAuthResult`).

use serde::{Deserialize, Serialize};

use crate::kernel::error::MoyasarError;

/// Validated `ca_`-prefixed identifier for a standalone 3D Secure card authentication.
///
/// # Examples
///
/// ```
/// use some_moyasar::kernel::error::MoyasarError;
/// use some_moyasar::resources::card_auths::models::CardAuthId;
///
/// let id = CardAuthId::new("ca_9f8e7d6c").expect("valid ca_ id");
/// assert_eq!(id.as_str(), "ca_9f8e7d6c");
/// assert_eq!(id.to_string(), "ca_9f8e7d6c");
///
/// assert!(matches!(CardAuthId::new("ca_"), Err(MoyasarError::InvalidCardAuthId)));
/// assert!(matches!(CardAuthId::new("tok_123"), Err(MoyasarError::InvalidCardAuthId)));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CardAuthId(String);

impl CardAuthId {
    /// Validates and constructs a [`CardAuthId`] from a raw string.
    ///
    /// Requires a `ca_` prefix followed by a non-empty suffix.
    ///
    /// # Errors
    ///
    /// Returns [`MoyasarError::InvalidCardAuthId`] if `raw` does not start with `ca_`
    /// or has an empty suffix after `ca_`.
    pub fn new(raw: impl Into<String>) -> Result<Self, MoyasarError> {
        let raw = raw.into();
        match raw.strip_prefix("ca_") {
            Some(suffix) if !suffix.is_empty() => Ok(Self(raw)),
            _ => Err(MoyasarError::InvalidCardAuthId),
        }
    }

    /// Returns the underlying `ca_...` identifier slice without allocating.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for CardAuthId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CardAuthId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Lifecycle status of a standalone 3D Secure card authentication.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::card_auths::models::CardAuthStatus;
///
/// let status: CardAuthStatus = serde_json::from_str(r#""authenticated""#).unwrap();
/// assert_eq!(status, CardAuthStatus::Authenticated);
/// assert_eq!(status.as_str(), "authenticated");
///
/// let future_status: CardAuthStatus = serde_json::from_str(r#""expired""#).unwrap();
/// assert_eq!(future_status, CardAuthStatus::Unknown);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardAuthStatus {
    /// Authentication session created; awaiting cardholder 3DS completion.
    Initiated,
    /// Cardholder authentication succeeded and MPI cryptographic data is available.
    Authenticated,
    /// Cardholder authentication failed or was rejected by the ACS.
    Failed,
    /// Forward-compatible fallback for unrecognized status strings.
    #[serde(other)]
    Unknown,
}

impl CardAuthStatus {
    const LABELS: [(Self, &'static str); 4] = [
        (Self::Initiated, "initiated"),
        (Self::Authenticated, "authenticated"),
        (Self::Failed, "failed"),
        (Self::Unknown, "unknown"),
    ];

    /// Returns the lowercase wire representation of this authentication status.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        Self::LABELS
            .iter()
            .find(|(status, _)| status == self)
            .map_or("unknown", |(_, label)| *label)
    }
}

impl std::fmt::Display for CardAuthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Cryptographic MPI outcome returned by Moyasar after 3D Secure authentication.
///
/// Implements a custom [`std::fmt::Debug`] that redacts `authentication_value` (CAVV)
/// as `"[REDACTED]"` to prevent sensitive cryptographic data from leaking into logs.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::card_auths::models::CardAuthResult;
///
/// let json = r#"{
///     "eci": "05",
///     "authentication_value": "AAABBIIFmAAAAAAAAAAAAAAAAAA=",
///     "ds_transaction_id": "f2c3d4e5-1111-2222-3333-444455556666",
///     "version": "2.2.0",
///     "transaction_status": "Y",
///     "auth_scheme": "visa",
///     "is_frictionless": true
/// }"#;
///
/// let result: CardAuthResult = serde_json::from_str(json).unwrap();
/// assert_eq!(result.eci.as_deref(), Some("05"));
/// assert_eq!(result.is_frictionless, Some(true));
///
/// let debug_output = format!("{result:?}");
/// assert!(debug_output.contains("[REDACTED]"));
/// assert!(!debug_output.contains("AAABBIIFmAAAAAAAAAAAAAAAAAA="));
/// ```
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct CardAuthResult {
    /// Electronic Commerce Indicator (ECI) assigned by the 3DS Directory Server.
    pub eci: Option<String>,
    /// Cardholder Authentication Verification Value (CAVV / `authentication_value`).
    ///
    /// Redacted as `"[REDACTED]"` in [`std::fmt::Debug`] output.
    pub authentication_value: Option<String>,
    /// Directory Server transaction identifier (`ds_transaction_id`).
    pub ds_transaction_id: Option<String>,
    /// 3D Secure protocol version (for example, `"2.1.0"` or `"2.2.0"`).
    pub version: Option<String>,
    /// Scheme-level transaction status (`"Y"`, `"N"`, `"A"`, `"U"`, etc.).
    pub transaction_status: Option<String>,
    /// Card network or authentication scheme (for example, `"visa"` or `"mastercard"`).
    pub auth_scheme: Option<String>,
    /// Whether authentication completed frictionlessly without an interactive challenge.
    pub is_frictionless: Option<bool>,
}

impl std::fmt::Debug for CardAuthResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CardAuthResult")
            .field("eci", &self.eci)
            .field(
                "authentication_value",
                &self.authentication_value.as_ref().map(|_| "[REDACTED]"),
            )
            .field("ds_transaction_id", &self.ds_transaction_id)
            .field("version", &self.version)
            .field("transaction_status", &self.transaction_status)
            .field("auth_scheme", &self.auth_scheme)
            .field("is_frictionless", &self.is_frictionless)
            .finish()
    }
}

/// Standalone 3D Secure card authentication resource returned by `/v1/card_auths`.
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::card_auths::models::{CardAuthStatus, CardAuthentication};
///
/// let json = r#"{
///     "id": "ca_01h2x3y4z5",
///     "status": "initiated",
///     "transaction_url": "https://api.moyasar.com/v1/card_auth/ca_01h2x3y4z5/prepare",
///     "result": null
/// }"#;
///
/// let auth: CardAuthentication = serde_json::from_str(json).unwrap();
/// assert_eq!(auth.id.as_str(), "ca_01h2x3y4z5");
/// assert_eq!(auth.status, CardAuthStatus::Initiated);
/// assert!(auth.result.is_none());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CardAuthentication {
    /// Unique `ca_`-prefixed identifier for this standalone 3DS authentication.
    pub id: CardAuthId,
    /// Current lifecycle status (`initiated`, `authenticated`, `failed`, or `unknown`).
    pub status: CardAuthStatus,
    /// Hosted 3DS preparation URL (`/v1/card_auth/:id/prepare`) for customer redirect.
    pub transaction_url: Option<String>,
    /// Cryptographic MPI authentication outcome populated once 3DS completes.
    pub result: Option<CardAuthResult>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_auth_id_validates_prefix_and_non_empty_suffix() {
        let valid = CardAuthId::new("ca_abc123").expect("valid ca_ prefix and suffix");
        assert_eq!(valid.as_str(), "ca_abc123");
        assert_eq!(valid.as_ref(), "ca_abc123");
        assert_eq!(valid.to_string(), "ca_abc123");

        assert!(matches!(
            CardAuthId::new("ca_"),
            Err(MoyasarError::InvalidCardAuthId)
        ));
        assert!(matches!(
            CardAuthId::new(""),
            Err(MoyasarError::InvalidCardAuthId)
        ));
        assert!(matches!(
            CardAuthId::new("tok_abc123"),
            Err(MoyasarError::InvalidCardAuthId)
        ));
    }

    #[test]
    fn card_auth_status_maps_all_variants_and_unknown_fallback() {
        assert_eq!(CardAuthStatus::Initiated.as_str(), "initiated");
        assert_eq!(CardAuthStatus::Authenticated.as_str(), "authenticated");
        assert_eq!(CardAuthStatus::Failed.as_str(), "failed");
        assert_eq!(CardAuthStatus::Unknown.as_str(), "unknown");
        assert_eq!(CardAuthStatus::Authenticated.to_string(), "authenticated");

        let initiated: CardAuthStatus = serde_json::from_str(r#""initiated""#).unwrap();
        let authenticated: CardAuthStatus = serde_json::from_str(r#""authenticated""#).unwrap();
        let failed: CardAuthStatus = serde_json::from_str(r#""failed""#).unwrap();
        let unknown: CardAuthStatus = serde_json::from_str(r#""pending_review""#).unwrap();

        assert_eq!(initiated, CardAuthStatus::Initiated);
        assert_eq!(authenticated, CardAuthStatus::Authenticated);
        assert_eq!(failed, CardAuthStatus::Failed);
        assert_eq!(unknown, CardAuthStatus::Unknown);
    }

    #[test]
    fn tc_4_3_0_02_deserializes_card_authentication_with_full_mpi_result() {
        let fixture = r#"{
            "id": "ca_9a8b7c6d5e",
            "status": "authenticated",
            "transaction_url": "https://api.moyasar.com/v1/card_auth/ca_9a8b7c6d5e/prepare",
            "result": {
                "eci": "05",
                "authentication_value": "kH9z8x7c6v5b4n3m2l1k0j==",
                "ds_transaction_id": "11111111-2222-3333-4444-555555555555",
                "version": "2.2.0",
                "transaction_status": "Y",
                "auth_scheme": "visa",
                "is_frictionless": true
            }
        }"#;

        let auth: CardAuthentication = serde_json::from_str(fixture).unwrap();
        assert_eq!(auth.id.as_str(), "ca_9a8b7c6d5e");
        assert_eq!(auth.status, CardAuthStatus::Authenticated);
        assert_eq!(
            auth.transaction_url.as_deref(),
            Some("https://api.moyasar.com/v1/card_auth/ca_9a8b7c6d5e/prepare")
        );

        let result = auth.result.expect("result should be present");
        assert_eq!(result.eci.as_deref(), Some("05"));
        assert_eq!(
            result.authentication_value.as_deref(),
            Some("kH9z8x7c6v5b4n3m2l1k0j==")
        );
        assert_eq!(
            result.ds_transaction_id.as_deref(),
            Some("11111111-2222-3333-4444-555555555555")
        );
        assert_eq!(result.version.as_deref(), Some("2.2.0"));
        assert_eq!(result.transaction_status.as_deref(), Some("Y"));
        assert_eq!(result.auth_scheme.as_deref(), Some("visa"));
        assert_eq!(result.is_frictionless, Some(true));
    }

    #[test]
    fn tc_4_3_0_03_distinguishes_frictionless_and_challenged_flows() {
        let frictionless_json = r#"{
            "eci": "05",
            "authentication_value": "cavv_frictionless",
            "ds_transaction_id": "ds-tx-001",
            "version": "2.2.0",
            "transaction_status": "Y",
            "auth_scheme": "mastercard",
            "is_frictionless": true
        }"#;
        let challenged_json = r#"{
            "eci": "02",
            "authentication_value": "cavv_challenged",
            "ds_transaction_id": "ds-tx-002",
            "version": "2.1.0",
            "transaction_status": "C",
            "auth_scheme": "mada",
            "is_frictionless": false
        }"#;

        let frictionless: CardAuthResult = serde_json::from_str(frictionless_json).unwrap();
        let challenged: CardAuthResult = serde_json::from_str(challenged_json).unwrap();

        assert_eq!(frictionless.is_frictionless, Some(true));
        assert_eq!(frictionless.transaction_status.as_deref(), Some("Y"));
        assert_eq!(challenged.is_frictionless, Some(false));
        assert_eq!(challenged.transaction_status.as_deref(), Some("C"));
    }

    #[test]
    fn tc_4_3_0_law_redacts_authentication_value_in_debug_output() {
        let secret_cavv = "SUPER_SECRET_CAVV_VALUE_XYZ123==";
        let auth = CardAuthentication {
            id: CardAuthId::new("ca_law_test").unwrap(),
            status: CardAuthStatus::Authenticated,
            transaction_url: Some(
                "https://api.moyasar.com/v1/card_auth/ca_law_test/prepare".into(),
            ),
            result: Some(CardAuthResult {
                eci: Some("05".into()),
                authentication_value: Some(secret_cavv.into()),
                ds_transaction_id: Some("ds-law-001".into()),
                version: Some("2.2.0".into()),
                transaction_status: Some("Y".into()),
                auth_scheme: Some("visa".into()),
                is_frictionless: Some(true),
            }),
        };

        let formatted = format!("{auth:?}");
        assert!(formatted.contains("[REDACTED]"));
        assert!(!formatted.contains(secret_cavv));

        let empty_result = CardAuthResult {
            eci: None,
            authentication_value: None,
            ds_transaction_id: None,
            version: None,
            transaction_status: None,
            auth_scheme: None,
            is_frictionless: None,
        };
        let empty_formatted = format!("{empty_result:?}");
        assert!(empty_formatted.contains("authentication_value: None"));
        assert!(!empty_formatted.contains("[REDACTED]"));
    }
}
