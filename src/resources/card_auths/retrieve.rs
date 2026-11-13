//! Standalone 3DS card authentication retrieval, MPI extraction, and payment bridging.

use core::fmt;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::MoyasarError;
use crate::kernel::transport::execute_with_retry;
use crate::resources::card_auths::CardAuthsService;
use crate::resources::card_auths::models::{CardAuthId, CardAuthResult, CardAuthentication};

/// Extracted 3DS MPI cryptographic parameters (`eci`, `authentication_value`, `ds_transaction_id`).
///
/// Implements a custom [`fmt::Debug`] that redacts `authentication_value` (CAVV)
/// to satisfy PCI-DSS 4.0 and `TC-4.3.0-LAW`.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MpiParameters {
    /// Electronic Commerce Indicator returned by the directory server or ACS.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eci: Option<String>,
    /// Cardholder Authentication Verification Value (CAVV / AAV); redacted in `Debug`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authentication_value: Option<String>,
    /// 3DS2 Directory Server Transaction ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ds_transaction_id: Option<String>,
}

impl MpiParameters {
    /// Returns `true` when `eci`, `authentication_value`, and `ds_transaction_id` are all present.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.eci.is_some()
            && self.authentication_value.is_some()
            && self.ds_transaction_id.is_some()
    }
}

impl fmt::Debug for MpiParameters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MpiParameters")
            .field("eci", &self.eci)
            .field(
                "authentication_value",
                &self.authentication_value.as_ref().map(|_| "[REDACTED]"),
            )
            .field("ds_transaction_id", &self.ds_transaction_id)
            .finish()
    }
}

/// External 3DS MPI parameters forwarded verbatim under `source.card_auth_data` (`AC-US-4.3.0-05`).
///
/// Implements a custom [`fmt::Debug`] that redacts `authentication_value` and any `cavv`
/// entry in `extra` so cryptographic authentication values are never logged in plaintext.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalMpiData {
    /// Electronic Commerce Indicator (e.g., `"05"` or `"02"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eci: Option<String>,
    /// Cardholder Authentication Verification Value (CAVV); redacted in `Debug`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authentication_value: Option<String>,
    /// 3DS2 Directory Server Transaction ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ds_transaction_id: Option<String>,
    /// 3DS protocol version (e.g., `"2.2.0"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Scheme transaction status (e.g., `"Y"`, `"A"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_status: Option<String>,
    /// Additional scheme-specific MPI fields serialized verbatim.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

fn redacted_extra_debug(extra: &BTreeMap<String, serde_json::Value>) -> BTreeMap<&str, String> {
    extra
        .iter()
        .map(|(k, v)| {
            let val = if k == "cavv" || k == "authentication_value" {
                "[REDACTED]".to_owned()
            } else {
                v.to_string()
            };
            (k.as_str(), val)
        })
        .collect()
}

impl fmt::Debug for ExternalMpiData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let extra_redacted = redacted_extra_debug(&self.extra);
        f.debug_struct("ExternalMpiData")
            .field("eci", &self.eci)
            .field(
                "authentication_value",
                &self.authentication_value.as_ref().map(|_| "[REDACTED]"),
            )
            .field("ds_transaction_id", &self.ds_transaction_id)
            .field("version", &self.version)
            .field("transaction_status", &self.transaction_status)
            .field("extra", &extra_redacted)
            .finish()
    }
}

/// Bridge payload for passing standalone or external 3DS authentication into payment creation.
///
/// Supports both `card_auth_id` (`AC-US-4.3.0-04`) and `card_auth_data` (`AC-US-4.3.0-05`),
/// while redacting `authentication_value` in [`fmt::Debug`].
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::card_auths::models::CardAuthId;
/// use some_moyasar::resources::card_auths::retrieve::{CardAuthBridgeData, ExternalMpiData};
///
/// let id = CardAuthId::new("ca_abc123").unwrap();
/// let by_id = CardAuthBridgeData::from_card_auth_id(id);
/// let id_json = serde_json::to_value(&by_id).unwrap();
/// assert_eq!(id_json["card_auth_id"], "ca_abc123");
/// assert!(id_json.get("card_auth_data").is_none());
///
/// let mpi = ExternalMpiData {
///     eci: Some("05".to_owned()),
///     authentication_value: Some("cavv_secret_token".to_owned()),
///     ds_transaction_id: Some("ds_tx_999".to_owned()),
///     ..Default::default()
/// };
/// let by_data = CardAuthBridgeData::from_card_auth_data(mpi);
/// let data_json = serde_json::to_value(&by_data).unwrap();
/// assert_eq!(data_json["card_auth_data"]["eci"], "05");
/// assert!(!format!("{by_data:?}").contains("cavv_secret_token"));
/// ```
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardAuthBridgeData {
    /// Moyasar standalone card authentication identifier (`ca_...`) for `AC-US-4.3.0-04`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_auth_id: Option<CardAuthId>,
    /// External 3DS MPI parameters forwarded verbatim for `AC-US-4.3.0-05`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_auth_data: Option<ExternalMpiData>,
}

impl CardAuthBridgeData {
    /// Creates a bridge payload referencing an existing Moyasar `card_auth_id` (`AC-US-4.3.0-04`).
    #[must_use]
    pub const fn from_card_auth_id(card_auth_id: CardAuthId) -> Self {
        Self {
            card_auth_id: Some(card_auth_id),
            card_auth_data: None,
        }
    }

    /// Creates a bridge payload carrying external `card_auth_data` (`AC-US-4.3.0-05`).
    #[must_use]
    pub const fn from_card_auth_data(card_auth_data: ExternalMpiData) -> Self {
        Self {
            card_auth_id: None,
            card_auth_data: Some(card_auth_data),
        }
    }

    /// Creates a `card_auth_data` bridge payload from extracted [`MpiParameters`].
    #[must_use]
    pub fn from_mpi(mpi: MpiParameters) -> Self {
        Self::from_card_auth_data(ExternalMpiData {
            eci: mpi.eci,
            authentication_value: mpi.authentication_value,
            ds_transaction_id: mpi.ds_transaction_id,
            ..Default::default()
        })
    }
}

impl fmt::Debug for CardAuthBridgeData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CardAuthBridgeData")
            .field("card_auth_id", &self.card_auth_id)
            .field("card_auth_data", &self.card_auth_data)
            .finish()
    }
}

/// Extracts [`MpiParameters`] (`eci`, `authentication_value`, `ds_transaction_id`)
/// from a [`CardAuthResult`].
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::card_auths::models::CardAuthResult;
/// use some_moyasar::resources::card_auths::retrieve::extract_mpi_result;
///
/// let result: CardAuthResult = serde_json::from_str(r#"{
///     "eci": "05",
///     "authentication_value": "cavv_secret_value",
///     "ds_transaction_id": "ds_123"
/// }"#).unwrap();
///
/// let mpi = extract_mpi_result(&result);
/// assert!(mpi.is_complete());
/// assert_eq!(mpi.eci.as_deref(), Some("05"));
/// assert!(!format!("{mpi:?}").contains("cavv_secret_value"));
/// ```
#[must_use]
pub fn extract_mpi_result(result: &CardAuthResult) -> MpiParameters {
    MpiParameters {
        eci: result.eci.clone(),
        authentication_value: result.authentication_value.clone(),
        ds_transaction_id: result.ds_transaction_id.clone(),
    }
}

/// Extracts [`MpiParameters`] from a [`CardAuthentication`] if its `result` block is present.
#[must_use]
pub fn extract_mpi_from_auth(auth: &CardAuthentication) -> Option<MpiParameters> {
    auth.result.as_ref().map(extract_mpi_result)
}

impl<'a, K: SecretAuthKey> CardAuthsService<'a, K> {
    /// Retrieves a standalone 3DS card authentication by ID (`GET /v1/card_auths/{id}`).
    ///
    /// Requires a [`SecretAuthKey`]. Passing a [`PublishableKey`](crate::kernel::auth::PublishableKey)
    /// is rejected at compile time:
    ///
    /// ```compile_fail
    /// use some_moyasar::prelude::{MoyasarClient, PublishableKey};
    /// use some_moyasar::resources::card_auths::models::CardAuthId;
    ///
    /// # async fn doc_check() {
    /// let pk = PublishableKey::new("pk_test_abc123").unwrap();
    /// let client = MoyasarClient::new(pk).unwrap();
    /// let id = CardAuthId::new("ca_123").unwrap();
    /// let _ = client.card_auths().retrieve(&id).await;
    /// # }
    /// ```
    pub async fn retrieve(&self, id: &CardAuthId) -> Result<CardAuthentication, MoyasarError> {
        let cfg = self.client().config();
        retrieve_from_base_url(self.client().http(), &cfg.base_url, cfg.max_network_retries, id)
            .await
    }
}

async fn retrieve_from_base_url(
    http: &reqwest::Client,
    base_url: &str,
    max_network_retries: u32,
    id: &CardAuthId,
) -> Result<CardAuthentication, MoyasarError> {
    let base = base_url.trim_end_matches('/');
    let url = format!("{base}/v1/card_auths/{id}");
    execute_with_retry(max_network_retries, || send_retrieve_once(http, &url)).await
}

async fn send_retrieve_once(
    http: &reqwest::Client,
    url: &str,
) -> Result<CardAuthentication, MoyasarError> {
    let response = http.get(url).send().await.map_err(map_reqwest_error)?;
    let status = response.status();
    let body = response.text().await.map_err(map_reqwest_error)?;

    if !status.is_success() {
        return Err(MoyasarError::from_api_response(status.as_u16(), &body));
    }

    serde_json::from_str(&body).map_err(|err| MoyasarError::ServerError {
        status: status.as_u16(),
        message: format!("failed to decode CardAuthentication response: {err}"),
    })
}

fn map_reqwest_error(err: reqwest::Error) -> MoyasarError {
    if err.is_timeout() {
        MoyasarError::Timeout
    } else {
        MoyasarError::Network(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::error::ApiErrorType;
    use crate::kernel::transport::build_default_headers;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_http_client(secret: &str) -> reqwest::Client {
        let key = SecretKey::new(secret).expect("valid test secret key");
        let headers = build_default_headers(&key).expect("default headers");
        reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .expect("reqwest client")
    }

    #[tokio::test]
    async fn retrieves_authenticated_card_auth_and_extracts_mpi() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/card_auths/ca_test_123"))
            .and(header("moyasar-version", "2024-01-01"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "ca_test_123",
                "status": "authenticated",
                "transaction_url": null,
                "result": {
                    "eci": "05",
                    "authentication_value": "AAABBCCCDDD_CAVV_SECRET",
                    "ds_transaction_id": "ds-tx-987",
                    "version": "2.2.0",
                    "transaction_status": "Y",
                    "auth_scheme": "Visa",
                    "is_frictionless": true
                }
            })))
            .mount(&server)
            .await;

        let http = test_http_client("sk_test_wiremock");
        let id = CardAuthId::new("ca_test_123").expect("valid card auth id");
        let auth = retrieve_from_base_url(&http, &server.uri(), 1, &id)
            .await
            .expect("retrieve succeeds");

        assert_eq!(auth.id.to_string(), "ca_test_123");
        let mpi = extract_mpi_from_auth(&auth).expect("mpi result present");
        assert!(mpi.is_complete());
        assert_eq!(mpi.eci.as_deref(), Some("05"));
        assert_eq!(
            mpi.authentication_value.as_deref(),
            Some("AAABBCCCDDD_CAVV_SECRET")
        );
        assert_eq!(mpi.ds_transaction_id.as_deref(), Some("ds-tx-987"));

        let dbg = format!("{mpi:?}");
        assert!(dbg.contains("[REDACTED]"));
        assert!(!dbg.contains("AAABBCCCDDD_CAVV_SECRET"));
    }

    #[tokio::test]
    async fn retrieve_maps_404_to_record_not_found_without_retrying() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/card_auths/ca_missing"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "type": "record_not_found",
                "message": "Card auth not found"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let http = test_http_client("sk_test_wiremock");
        let id = CardAuthId::new("ca_missing").expect("valid id");
        let err = retrieve_from_base_url(&http, &server.uri(), 2, &id)
            .await
            .expect_err("404 must fail");

        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::RecordNotFound,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn retrieve_retries_503_and_succeeds_on_recovery() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/card_auths/ca_retry_1"))
            .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/card_auths/ca_retry_1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "ca_retry_1",
                "status": "failed",
                "transaction_url": null,
                "result": null
            })))
            .mount(&server)
            .await;

        let http = test_http_client("sk_test_wiremock");
        let id = CardAuthId::new("ca_retry_1").expect("valid id");
        let auth = retrieve_from_base_url(&http, &server.uri(), 2, &id)
            .await
            .expect("recovers after 503");

        assert_eq!(auth.id.to_string(), "ca_retry_1");
        assert!(extract_mpi_from_auth(&auth).is_none());
    }

    #[test]
    fn tc_4_3_0_04_serializes_card_auth_id_bridge() {
        let id = CardAuthId::new("ca_abc").expect("valid card auth id");
        let bridge = CardAuthBridgeData::from_card_auth_id(id);
        let json = serde_json::to_value(&bridge).expect("serializes bridge");

        assert_eq!(
            json,
            serde_json::json!({
                "card_auth_id": "ca_abc"
            })
        );
    }

    #[test]
    fn tc_4_3_0_05_serializes_card_auth_data_and_redacts_debug() {
        let mut extra = BTreeMap::new();
        extra.insert("cavv".to_owned(), serde_json::json!("raw_cavv_secret"));
        let data = ExternalMpiData {
            eci: Some("05".to_owned()),
            authentication_value: Some("sensitive_auth_val_xyz".to_owned()),
            ds_transaction_id: Some("ds_tx_456".to_owned()),
            version: Some("2.2.0".to_owned()),
            transaction_status: Some("Y".to_owned()),
            extra,
        };
        let bridge = CardAuthBridgeData::from_card_auth_data(data);
        let json = serde_json::to_value(&bridge).expect("serializes bridge");

        assert_eq!(json["card_auth_data"]["eci"], "05");
        assert_eq!(
            json["card_auth_data"]["authentication_value"],
            "sensitive_auth_val_xyz"
        );
        assert_eq!(json["card_auth_data"]["ds_transaction_id"], "ds_tx_456");
        assert_eq!(json["card_auth_data"]["cavv"], "raw_cavv_secret");

        let dbg = format!("{bridge:?}");
        assert!(dbg.contains("[REDACTED]"));
        assert!(!dbg.contains("sensitive_auth_val_xyz"));
        assert!(!dbg.contains("raw_cavv_secret"));
    }
}
