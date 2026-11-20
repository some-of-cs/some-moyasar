//! Unified [`PayoutCapability`] adapter backed by `client.payouts()` (`sarie`, `ips`, `internal`).

use core::fmt;
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::kernel::auth::SecretKey;
use crate::kernel::client::MoyasarClient;
use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::metadata::Metadata;
use crate::resources::payouts::{
    BankDestination, BulkPayoutEntry, CreateBulkPayoutParams, CreatePayoutParams, Payout,
    PayoutAccountId, PayoutChannel, PayoutDestination, PayoutId, PayoutPurpose, PayoutStatus,
    WalletDestination,
};

const PAYOUT_CHANNEL_STRINGS: &[(PayoutChannel, &str)] = &[
    (PayoutChannel::Internal, "internal"),
    (PayoutChannel::Ips, "ips"),
    (PayoutChannel::Sarie, "sarie"),
    (PayoutChannel::Unknown, "unknown"),
];

const PAYOUT_STATUS_STRINGS: &[(PayoutStatus, &str)] = &[
    (PayoutStatus::Queued, "queued"),
    (PayoutStatus::Initiated, "initiated"),
    (PayoutStatus::Paid, "paid"),
    (PayoutStatus::Failed, "failed"),
    (PayoutStatus::Canceled, "canceled"),
    (PayoutStatus::Returned, "returned"),
];

/// Cross-gateway beneficiary destination for outbound payout disbursements.
///
/// Sensitive beneficiary identifiers (`iban`, `mobile`) are redacted as `[REDACTED]`
/// in [`fmt::Debug`] output.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum UnifiedPayoutDestination {
    /// Bank account transfer destination (`internal`, `ips`, or `sarie`).
    Bank {
        /// Beneficiary Saudi IBAN (`SA` followed by 22 ASCII digits).
        iban: String,
        /// Beneficiary legal name (required for SAMA compliance).
        name: String,
        /// Beneficiary Saudi mobile number (`05XXXXXXXX`, `+9665XXXXXXXX`, or `9665XXXXXXXX`).
        mobile: String,
        /// Two-letter ISO 3166-1 alpha-2 country code (e.g., `"SA"`).
        country: String,
        /// Beneficiary city name (e.g., `"Riyadh"`).
        city: String,
    },
    /// Mobile wallet transfer destination.
    Wallet {
        /// Beneficiary Saudi mobile number (`05XXXXXXXX`, `+9665XXXXXXXX`, or `9665XXXXXXXX`).
        mobile: String,
    },
}

impl fmt::Debug for UnifiedPayoutDestination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bank {
                name,
                country,
                city,
                ..
            } => f
                .debug_struct("Bank")
                .field("iban", &"[REDACTED]")
                .field("name", name)
                .field("mobile", &"[REDACTED]")
                .field("country", country)
                .field("city", city)
                .finish(),
            Self::Wallet { .. } => f
                .debug_struct("Wallet")
                .field("mobile", &"[REDACTED]")
                .finish(),
        }
    }
}

/// Beneficiary country and city for a bank account payout destination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankLocation {
    /// Two-letter ISO 3166-1 alpha-2 country code (e.g., `"SA"`).
    pub country: String,
    /// Beneficiary city name (e.g., `"Riyadh"`).
    pub city: String,
}

impl BankLocation {
    /// Creates a new [`BankLocation`] from an ISO 3166-1 alpha-2 `country` code and `city`.
    #[must_use]
    pub fn new(country: impl Into<String>, city: impl Into<String>) -> Self {
        Self {
            country: country.into(),
            city: city.into(),
        }
    }
}

impl UnifiedPayoutDestination {
    /// Constructs a [`UnifiedPayoutDestination::Bank`] destination.
    #[must_use]
    pub fn bank(
        iban: impl Into<String>,
        name: impl Into<String>,
        mobile: impl Into<String>,
        location: BankLocation,
    ) -> Self {
        Self::Bank {
            iban: iban.into(),
            name: name.into(),
            mobile: mobile.into(),
            country: location.country,
            city: location.city,
        }
    }

    /// Constructs a [`UnifiedPayoutDestination::Wallet`] destination.
    #[must_use]
    pub fn wallet(mobile: impl Into<String>) -> Self {
        Self::Wallet {
            mobile: mobile.into(),
        }
    }

    /// Converts this unified destination into a Moyasar [`PayoutDestination`].
    #[must_use]
    pub fn into_moyasar(self) -> PayoutDestination {
        match self {
            Self::Bank {
                iban,
                name,
                mobile,
                country,
                city,
            } => PayoutDestination::Bank(BankDestination {
                iban,
                name,
                mobile,
                country,
                city,
            }),
            Self::Wallet { mobile } => PayoutDestination::Wallet(WalletDestination { mobile }),
        }
    }
}

/// Cross-gateway request parameters for initiating an outbound payout disbursement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedPayoutRequest {
    /// Source Moyasar payout account identifier (e.g., `"pa_..."`).
    pub source_account_id: String,
    /// Disbursement amount in minor currency units (e.g., Halalas for SAR).
    pub amount_minor: u64,
    /// SAMA-regulated `snake_case` purpose code (e.g., `"payment_to_merchant"`).
    pub purpose: String,
    /// Beneficiary bank account or mobile wallet destination.
    pub destination: UnifiedPayoutDestination,
    /// Optional 16-digit ASCII numeric reconciliation sequence number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<String>,
    /// Optional human-readable remittance comment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Optional validated merchant metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

impl UnifiedPayoutRequest {
    /// Creates a new [`UnifiedPayoutRequest`] with required disbursement parameters.
    #[must_use]
    pub fn new(
        source_account_id: impl Into<String>,
        amount_minor: u64,
        purpose: impl Into<String>,
        destination: UnifiedPayoutDestination,
    ) -> Self {
        Self {
            source_account_id: source_account_id.into(),
            amount_minor,
            purpose: purpose.into(),
            destination,
            sequence_number: None,
            comment: None,
            metadata: None,
        }
    }

    /// Attaches a 16-digit ASCII numeric reconciliation `sequence_number`.
    #[must_use]
    pub fn sequence_number(mut self, sequence_number: impl Into<String>) -> Self {
        self.sequence_number = Some(sequence_number.into());
        self
    }

    /// Attaches an optional remittance comment.
    #[must_use]
    pub fn comment(mut self, comment: impl Into<String>) -> Self {
        self.comment = Some(comment.into());
        self
    }

    /// Attaches validated key/value [`Metadata`].
    #[must_use]
    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    fn into_create_params(self) -> Result<CreatePayoutParams, MoyasarError> {
        let source_id = PayoutAccountId::new(self.source_account_id.as_str())?;
        let entry = self.into_bulk_entry()?;
        Ok(CreatePayoutParams {
            source_id,
            sequence_number: entry.sequence_number,
            amount: entry.amount,
            purpose: entry.purpose,
            destination: entry.destination,
            comment: entry.comment,
            metadata: entry.metadata,
        })
    }

    fn into_bulk_entry(self) -> Result<BulkPayoutEntry, MoyasarError> {
        let purpose = parse_payout_purpose(&self.purpose)?;
        Ok(BulkPayoutEntry {
            sequence_number: self.sequence_number,
            amount: self.amount_minor,
            purpose,
            destination: self.destination.into_moyasar(),
            comment: self.comment,
            metadata: self.metadata,
        })
    }
}

/// Unified payout disbursement response across payment gateways.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedPayoutResponse {
    /// Unique gateway payout identifier.
    pub id: String,
    /// Normalized payout transfer rail (`"internal"`, `"ips"`, `"sarie"`, or `"unknown"`).
    pub channel: String,
    /// Normalized payout status (`"queued"`, `"initiated"`, `"paid"`, `"failed"`, etc.).
    pub status: String,
    /// Disbursement amount in minor currency units (`u64`).
    pub amount_minor: u64,
    /// Optional human-readable status message or description.
    pub message: Option<String>,
    /// Optional failure or return reason when the disbursement did not settle.
    pub failure_reason: Option<String>,
}

/// Unified bulk payout disbursement response across payment gateways.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedBulkPayoutResponse {
    /// Created payout disbursement records for the submitted batch.
    pub payouts: Vec<UnifiedPayoutResponse>,
}

/// Companion `some-payment` orthogonal capability trait for outbound payout operations.
///
/// # Examples
///
/// ```
/// use some_moyasar::prelude::{MoyasarClient, SecretKey};
/// use some_moyasar::unified::PayoutCapability;
///
/// let key = SecretKey::new("sk_test_payout_cap_demo").unwrap();
/// let client = MoyasarClient::new(key).unwrap();
/// fn assert_payouts<P: PayoutCapability>(_p: &P) {}
/// assert_payouts(&client);
/// ```
pub trait PayoutCapability: Send + Sync {
    /// Concrete gateway error type.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Initiates a single outbound payout disbursement (`POST /v1/payouts`).
    fn disburse<'a>(
        &'a self,
        request: UnifiedPayoutRequest,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedPayoutResponse, Self::Error>> + Send + 'a>>;

    /// Initiates a batch of outbound payouts funded by `source_account_id` (`POST /v1/payouts/bulk`).
    fn disburse_bulk<'a>(
        &'a self,
        source_account_id: &'a str,
        requests: Vec<UnifiedPayoutRequest>,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedBulkPayoutResponse, Self::Error>> + Send + 'a>>;

    /// Retrieves the latest status of an outbound payout by `payout_id` (`GET /v1/payouts/{id}`).
    fn payout_status<'a>(
        &'a self,
        payout_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedPayoutResponse, Self::Error>> + Send + 'a>>;
}

type PayoutFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, MoyasarError>> + Send + 'a>>;

impl PayoutCapability for MoyasarClient<SecretKey> {
    type Error = MoyasarError;

    fn disburse<'a>(
        &'a self,
        request: UnifiedPayoutRequest,
    ) -> PayoutFuture<'a, UnifiedPayoutResponse> {
        Box::pin(async move {
            self.payouts()
                .create(&request.into_create_params()?)
                .await
                .map(map_unified_payout)
        })
    }

    fn disburse_bulk<'a>(
        &'a self,
        source_account_id: &'a str,
        requests: Vec<UnifiedPayoutRequest>,
    ) -> PayoutFuture<'a, UnifiedBulkPayoutResponse> {
        Box::pin(async move {
            let source_id = PayoutAccountId::new(source_account_id)?;
            let entries = requests
                .into_iter()
                .map(UnifiedPayoutRequest::into_bulk_entry)
                .collect::<Result<Vec<BulkPayoutEntry>, MoyasarError>>()?;
            let params = CreateBulkPayoutParams::new(source_id, entries);
            let batch = self.payouts().create_bulk(&params).await?;
            let payouts = batch.payouts.into_iter().map(map_unified_payout).collect();
            Ok(UnifiedBulkPayoutResponse { payouts })
        })
    }

    fn payout_status<'a>(&'a self, payout_id: &'a str) -> PayoutFuture<'a, UnifiedPayoutResponse> {
        Box::pin(async move {
            self.payouts()
                .retrieve(&PayoutId::new(payout_id)?)
                .await
                .map(map_unified_payout)
        })
    }
}

fn parse_payout_purpose(raw: &str) -> Result<PayoutPurpose, MoyasarError> {
    PayoutPurpose::from_str(raw).map_err(|err| MoyasarError::Api {
        error_type: ApiErrorType::InvalidRequestError,
        message: err.to_string(),
        errors: Vec::new(),
        response_code: None,
    })
}

fn lookup_label<T: PartialEq>(table: &[(T, &'static str)], key: &T) -> &'static str {
    table
        .iter()
        .find(|(variant, _)| variant == key)
        .map_or("unknown", |(_, label)| *label)
}

fn map_unified_payout(payout: Payout) -> UnifiedPayoutResponse {
    UnifiedPayoutResponse {
        id: payout.id.as_str().to_owned(),
        channel: lookup_label(PAYOUT_CHANNEL_STRINGS, &payout.channel).to_owned(),
        status: lookup_label(PAYOUT_STATUS_STRINGS, &payout.status).to_owned(),
        amount_minor: payout.amount,
        message: payout.message,
        failure_reason: payout.failure_reason,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn test_client(uri: &str) -> MoyasarClient<SecretKey> {
        MoyasarClient::builder(SecretKey::new("sk_test_payout_adapter").unwrap())
            .base_url(uri)
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn disburse_sarie_bank_and_payout_status_roundtrip() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/payouts"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "pout_sarie_01",
                "channel": "sarie",
                "status": "queued",
                "amount": 50000,
                "message": "Vendor settlement",
                "failure_reason": null
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/payouts/pout_sarie_01"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "pout_sarie_01",
                "channel": "sarie",
                "status": "paid",
                "amount": 50000,
                "message": "Vendor settlement",
                "failure_reason": null
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let dest = UnifiedPayoutDestination::bank(
            "SA0380000000608010167519",
            "Acme Logistics LLC",
            "0512345678",
            BankLocation::new("SA", "Riyadh"),
        );
        let debug_str = format!("{dest:?}");
        assert!(!debug_str.contains("SA0380000000608010167519"));
        assert!(!debug_str.contains("0512345678"));
        assert!(debug_str.contains("[REDACTED]"));

        let req = UnifiedPayoutRequest::new("pa_01HXYZ", 50_000, "payment_to_merchant", dest)
            .sequence_number("2026112000000001")
            .comment("Invoice #101");
        let created = client.disburse(req).await.unwrap();
        assert_eq!(created.id, "pout_sarie_01");
        assert_eq!(created.channel, "sarie");
        assert_eq!(created.status, "queued");
        assert_eq!(created.amount_minor, 50_000);

        let status = client.payout_status("pout_sarie_01").await.unwrap();
        assert_eq!(status.status, "paid");
        assert_eq!(status.channel, "sarie");
    }

    #[tokio::test]
    async fn disburse_bulk_dispatches_ips_and_internal_channels() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/payouts/bulk"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "payouts": [
                    {
                        "id": "pout_ips_01",
                        "channel": "ips",
                        "status": "initiated",
                        "amount": 10000,
                        "message": "Instant transfer",
                        "failure_reason": null
                    },
                    {
                        "id": "pout_internal_02",
                        "channel": "internal",
                        "status": "paid",
                        "amount": 25000,
                        "message": null,
                        "failure_reason": null
                    }
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = test_client(&server.uri());
        let batch = client
            .disburse_bulk(
                "pa_01HXYZ",
                vec![
                    UnifiedPayoutRequest::new(
                        "pa_01HXYZ",
                        10_000,
                        "payroll_benefits",
                        UnifiedPayoutDestination::wallet("+966512345678"),
                    ),
                    UnifiedPayoutRequest::new(
                        "pa_01HXYZ",
                        25_000,
                        "payment_to_merchant",
                        UnifiedPayoutDestination::wallet("0512345678"),
                    ),
                ],
            )
            .await
            .unwrap();

        assert_eq!(batch.payouts.len(), 2);
        assert_eq!(batch.payouts[0].channel, "ips");
        assert_eq!(batch.payouts[1].channel, "internal");
        assert_eq!(batch.payouts[1].status, "paid");
    }

    #[tokio::test]
    async fn disburse_rejects_unknown_purpose_and_zero_amount() {
        let server = MockServer::start().await;
        let client = test_client(&server.uri());

        let bad_purpose = UnifiedPayoutRequest::new(
            "pa_01HXYZ",
            10_000,
            "not_a_real_sama_purpose",
            UnifiedPayoutDestination::wallet("0512345678"),
        );
        let err = client.disburse(bad_purpose).await.unwrap_err();
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            }
        ));

        let zero_amt = UnifiedPayoutRequest::new(
            "pa_01HXYZ",
            0,
            "payroll_benefits",
            UnifiedPayoutDestination::wallet("0512345678"),
        );
        let err = client.disburse(zero_amt).await.unwrap_err();
        assert!(matches!(err, MoyasarError::InvalidPayoutAmount));
    }
}
