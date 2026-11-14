//! Single payout creation endpoint (`POST /v1/payouts`) (`US-5.2.0` Unit 4).
//!
//! Provides [`CreatePayoutParams`], [`validate_sequence_number`], and
//! [`PayoutsService::create`] / [`PayoutsService::create_at`] for initiating
//! single-recipient disbursements through Moyasar.

use serde::{Deserialize, Serialize};

use crate::kernel::auth::SecretAuthKey;
use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::metadata::Metadata;
use crate::kernel::transport::execute_with_retry;
use crate::resources::payouts::destinations::PayoutDestination;
use crate::resources::payouts::models::PayoutAccountId;
use crate::resources::payouts::payout_model::Payout;
use crate::resources::payouts::purpose::PayoutPurpose;
use crate::resources::payouts::service::PayoutsService;

/// Required exact length (in ASCII digits) for a payout `sequence_number`.
pub const PAYOUT_SEQUENCE_NUMBER_LEN: usize = 16;

/// Validates that `seq` is an exact 16-digit ASCII numeric string.
///
/// Returns `Ok(())` if and only if `seq.len() == 16` and every byte is an
/// ASCII decimal digit (`'0'..='9'`), otherwise returns
/// [`MoyasarError::InvalidPayoutAmount`].
///
/// # Examples
///
/// ```
/// use some_moyasar::resources::payouts::payouts_create::validate_sequence_number;
///
/// assert!(validate_sequence_number("2026111400000001").is_ok());
/// assert!(validate_sequence_number("12345").is_err());
/// assert!(validate_sequence_number("202611140000000A").is_err());
/// ```
pub fn validate_sequence_number(seq: &str) -> Result<(), MoyasarError> {
    if seq.len() == PAYOUT_SEQUENCE_NUMBER_LEN && seq.bytes().all(|b| b.is_ascii_digit()) {
        Ok(())
    } else {
        Err(MoyasarError::InvalidPayoutAmount)
    }
}

/// Request payload for creating a single payout via `POST /v1/payouts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatePayoutParams {
    /// Identifier of the source payout account funding the disbursement.
    pub source_id: PayoutAccountId,
    /// Optional 16-digit ASCII numeric sequence number for caller tracking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<String>,
    /// Disbursement amount in the smallest currency unit (must be `> 0`).
    pub amount: u64,
    /// SAMA-regulated purpose code describing the reason for the payout.
    pub purpose: PayoutPurpose,
    /// Destination bank account or digital wallet receiving the payout.
    pub destination: PayoutDestination,
    /// Optional human-readable comment attached to the payout.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Optional validated key/value metadata map.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

impl CreatePayoutParams {
    /// Validates all client-side invariants on [`CreatePayoutParams`].
    ///
    /// Enforces:
    /// - `self.amount > 0` (returning [`MoyasarError::InvalidPayoutAmount`] on `0`),
    /// - optional `self.sequence_number` via [`validate_sequence_number`], and
    /// - `self.destination.validate()?`.
    pub fn validate(&self) -> Result<(), MoyasarError> {
        if self.amount == 0 {
            return Err(MoyasarError::InvalidPayoutAmount);
        }
        if let Some(ref seq) = self.sequence_number {
            validate_sequence_number(seq)?;
        }
        self.destination.validate()?;
        Ok(())
    }
}

impl<'a, K: SecretAuthKey> PayoutsService<'a, K> {
    /// Creates a new single payout disbursement via `POST /v1/payouts`.
    ///
    /// Validates `params` locally via [`CreatePayoutParams::validate`] before
    /// dispatching the request using the configured client `base_url` and
    /// retry policy.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, MoyasarError, SecretKey};
    /// use some_moyasar::resources::payouts::payouts_create::CreatePayoutParams;
    ///
    /// # async fn run(params: CreatePayoutParams) -> Result<(), MoyasarError> {
    /// let key = SecretKey::new("sk_test_YOUR_SECRET_KEY")?;
    /// let client = MoyasarClient::new(key)?;
    /// let payout = client.payouts().create(&params).await?;
    /// let _ = payout;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create(&self, params: &CreatePayoutParams) -> Result<Payout, MoyasarError> {
        self.create_at(
            self.client().config().base_url.trim_end_matches('/'),
            params,
        )
        .await
    }

    /// Creates a new single payout disbursement against an explicit `base_url`.
    ///
    /// Validates `params` locally via [`CreatePayoutParams::validate`] and
    /// sends `POST {base_url}/v1/payouts` through [`execute_with_retry`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use some_moyasar::prelude::{MoyasarClient, MoyasarError, SecretKey};
    /// use some_moyasar::resources::payouts::payouts_create::CreatePayoutParams;
    ///
    /// # async fn run(params: CreatePayoutParams) -> Result<(), MoyasarError> {
    /// let key = SecretKey::new("sk_test_YOUR_SECRET_KEY")?;
    /// let client = MoyasarClient::new(key)?;
    /// let payout = client
    ///     .payouts()
    ///     .create_at("https://api.moyasar.com", &params)
    ///     .await?;
    /// let _ = payout;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_at(
        &self,
        base_url: &str,
        params: &CreatePayoutParams,
    ) -> Result<Payout, MoyasarError> {
        params.validate()?;
        let url = format!("{}/v1/payouts", base_url.trim_end_matches('/'));
        let http = self.client().http();
        let retries = self.client().config().max_network_retries;

        execute_with_retry(retries, || async {
            let response = http
                .post(&url)
                .json(params)
                .send()
                .await
                .map_err(MoyasarError::from)?;
            parse_payout_response(response).await
        })
        .await
    }
}

async fn parse_payout_response(response: reqwest::Response) -> Result<Payout, MoyasarError> {
    let status = response.status();
    let body = response.text().await.map_err(MoyasarError::from)?;
    if !status.is_success() {
        return Err(MoyasarError::from_api_response(status.as_u16(), &body));
    }
    serde_json::from_str::<Payout>(&body).map_err(|err| MoyasarError::Api {
        error_type: ApiErrorType::Unknown,
        message: format!("failed to decode Payout response: {err}"),
        errors: Vec::new(),
        response_code: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::auth::SecretKey;
    use crate::kernel::client::MoyasarClient;
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sample_params(amount: u64, sequence_number: Option<&str>) -> CreatePayoutParams {
        serde_json::from_value(json!({
            "source_id": "pa_01J9PAYACC0001",
            "sequence_number": sequence_number,
            "amount": amount,
            "purpose": "payroll_benefits",
            "destination": {
                "type": "bank",
                "iban": "SA0380000000608010167519",
                "name": "Acme Employee",
                "mobile": "+966500000001",
                "country": "SA",
                "city": "Riyadh"
            }
        }))
        .expect("valid CreatePayoutParams JSON fixture")
    }

    #[tokio::test]
    async fn tc_5_2_0_01_create_payout_posts_to_v1_payouts_and_parses_response() {
        let server = MockServer::start().await;
        let response_body = json!({
            "id": "po_01J9PAYOUT0001",
            "channel": "sarie",
            "source_id": "pa_01J9PAYACC0001",
            "sequence_number": "2026111400000001",
            "amount": 50_000,
            "currency": "SAR",
            "purpose": "payroll_benefits",
            "status": "initiated",
            "destination": {
                "type": "bank",
                "iban": "SA0380000000608010167519",
                "name": "Acme Employee",
                "mobile": "+966500000001",
                "country": "SA",
                "city": "Riyadh"
            },
            "created_at": "2026-11-14T06:00:00Z",
            "updated_at": "2026-11-14T06:00:00Z"
        });

        Mock::given(method("POST"))
            .and(path("/v1/payouts"))
            .and(body_partial_json(json!({
                "source_id": "pa_01J9PAYACC0001",
                "sequence_number": "2026111400000001",
                "amount": 50_000
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(response_body))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_payout_create").expect("valid secret key");
        let client = MoyasarClient::new(key).expect("client builds");
        let params = sample_params(50_000, Some("2026111400000001"));

        let payout = client
            .payouts()
            .create_at(&server.uri(), &params)
            .await
            .expect("single payout creation succeeds");

        let serialized = serde_json::to_value(&payout).expect("payout serializes");
        assert_eq!(serialized["id"], "po_01J9PAYOUT0001");
        assert_eq!(serialized["amount"], 50_000);
    }

    #[test]
    fn tc_5_2_0_05_sequence_number_validation_and_optional_field_serialization() {
        assert!(validate_sequence_number("2026111400000001").is_ok());

        let minimal = sample_params(10_000, None);
        assert!(minimal.validate().is_ok());
        let json_minimal = serde_json::to_value(&minimal).expect("serializes minimal params");
        assert!(json_minimal.get("sequence_number").is_none());
        assert!(json_minimal.get("comment").is_none());
        assert!(json_minimal.get("metadata").is_none());

        let mut with_opts = sample_params(10_000, Some("2026111400000002"));
        with_opts.comment = Some("November salary".to_owned());
        let mut meta = Metadata::new();
        meta.insert("batch_id", "nov_2026").expect("valid metadata");
        with_opts.metadata = Some(meta);

        assert!(with_opts.validate().is_ok());
        let json_full = serde_json::to_value(&with_opts).expect("serializes full params");
        assert_eq!(json_full["sequence_number"], "2026111400000002");
        assert_eq!(json_full["comment"], "November salary");
        assert_eq!(json_full["metadata"]["batch_id"], "nov_2026");
    }

    #[tokio::test]
    async fn rejects_zero_amount_before_sending_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/payouts"))
            .respond_with(ResponseTemplate::new(201))
            .expect(0)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_zero_amount").expect("valid secret key");
        let client = MoyasarClient::new(key).expect("client builds");
        let params = sample_params(0, Some("2026111400000001"));

        let err = client
            .payouts()
            .create_at(&server.uri(), &params)
            .await
            .expect_err("zero amount must be rejected");

        assert!(matches!(err, MoyasarError::InvalidPayoutAmount));
    }

    #[tokio::test]
    async fn rejects_invalid_16_digit_sequence_number() {
        assert!(matches!(
            validate_sequence_number("123456789012345"),
            Err(MoyasarError::InvalidPayoutAmount)
        ));
        assert!(matches!(
            validate_sequence_number("12345678901234567"),
            Err(MoyasarError::InvalidPayoutAmount)
        ));
        assert!(matches!(
            validate_sequence_number("202611140000000A"),
            Err(MoyasarError::InvalidPayoutAmount)
        ));

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/payouts"))
            .respond_with(ResponseTemplate::new(201))
            .expect(0)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_bad_seq").expect("valid secret key");
        let client = MoyasarClient::new(key).expect("client builds");
        let params = sample_params(25_000, Some("202611140000000X"));

        let err = client
            .payouts()
            .create_at(&server.uri(), &params)
            .await
            .expect_err("invalid sequence_number must be rejected");

        assert!(matches!(err, MoyasarError::InvalidPayoutAmount));
    }

    #[tokio::test]
    async fn does_not_retry_2xx_response_when_body_fails_to_decode() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/payouts"))
            .respond_with(ResponseTemplate::new(201).set_body_string("{not_valid_json"))
            .expect(1)
            .mount(&server)
            .await;

        let key = SecretKey::new("sk_test_payout_bad_201").expect("valid secret key");
        let client = MoyasarClient::new(key).expect("client builds");
        let params = sample_params(50_000, Some("2026111400000001"));

        let err = client
            .payouts()
            .create_at(&server.uri(), &params)
            .await
            .expect_err("malformed 201 body must return decode error");

        assert!(!err.is_retryable());
        assert!(matches!(
            err,
            MoyasarError::Api {
                error_type: ApiErrorType::Unknown,
                ..
            }
        ));
        let msg = err.to_string();
        assert!(msg.contains("failed to decode Payout response:"));
    }
}
