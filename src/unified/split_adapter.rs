//! Unified [`SplitSettlementCapability`] adapter backed by `client.payments().create(...)`.

use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kernel::auth::SecretKey;
use crate::kernel::client::MoyasarClient;
use crate::kernel::error::MoyasarError;
use crate::kernel::metadata::Metadata;
use crate::kernel::types::Currency;
use crate::resources::payments::{
    CreatePaymentParams, PaymentSourceRequest, PaymentSplitRequest, SplitRecipientType, TokenSource,
};

/// Cross-gateway recipient role for a marketplace split allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UnifiedSplitRecipientRole {
    /// Registered gateway entity account.
    Entity,
    /// Marketplace platform commission account.
    Platform,
    /// Vendor or sub-merchant beneficiary account.
    Beneficiary,
}

impl UnifiedSplitRecipientRole {
    /// Converts this unified role into a Moyasar [`SplitRecipientType`].
    #[must_use]
    pub const fn into_moyasar(self) -> SplitRecipientType {
        match self {
            Self::Entity => SplitRecipientType::Entity,
            Self::Platform => SplitRecipientType::Platform,
            Self::Beneficiary => SplitRecipientType::Beneficiary,
        }
    }
}

/// Cross-gateway split instruction attached to a marketplace payment request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedSplitRule {
    /// Split allocation in minor currency units (`u64`, must be `> 0`).
    pub amount_minor: u64,
    /// Role of the recipient receiving this split.
    pub role: UnifiedSplitRecipientRole,
    /// Gateway UUID of the recipient account.
    pub recipient_id: Uuid,
    /// Optional unique reconciliation reference persisted into settlement files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// Optional human-readable description for this split.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether this split absorbs gateway processing fees (at most one per payment).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_source: Option<bool>,
    /// Whether this split reverses proportionally on parent payment refund.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refundable: Option<bool>,
}

impl UnifiedSplitRule {
    /// Creates a new [`UnifiedSplitRule`] with required amount, role, and recipient UUID.
    #[must_use]
    pub const fn new(
        amount_minor: u64,
        role: UnifiedSplitRecipientRole,
        recipient_id: Uuid,
    ) -> Self {
        Self {
            amount_minor,
            role,
            recipient_id,
            reference: None,
            description: None,
            fee_source: None,
            refundable: None,
        }
    }

    /// Attaches a reconciliation `reference` to this split rule.
    #[must_use]
    pub fn reference(mut self, reference: impl Into<String>) -> Self {
        self.reference = Some(reference.into());
        self
    }

    /// Attaches a human-readable `description` to this split rule.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Designates whether this split absorbs processing fees.
    #[must_use]
    pub const fn fee_source(mut self, fee_source: bool) -> Self {
        self.fee_source = Some(fee_source);
        self
    }

    /// Controls proportional reversal when the parent payment is refunded.
    #[must_use]
    pub const fn refundable(mut self, refundable: bool) -> Self {
        self.refundable = Some(refundable);
        self
    }

    /// Converts this unified split rule into a Moyasar [`PaymentSplitRequest`].
    #[must_use]
    pub fn into_moyasar(self) -> PaymentSplitRequest {
        let mut req = PaymentSplitRequest::new(
            self.amount_minor,
            self.role.into_moyasar(),
            self.recipient_id,
        );
        req.reference = self.reference;
        req.description = self.description;
        req.fee_source = self.fee_source;
        if let Some(flag) = self.refundable {
            req.refundable = Some(flag);
        }
        req
    }
}

/// Cross-gateway request parameters for creating a marketplace payment with split settlement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedSplitPaymentRequest {
    /// Total payment amount in minor currency units (`u64`).
    pub amount_minor: u64,
    /// ISO 4217 currency code string (e.g., `"SAR"`).
    pub currency: String,
    /// Saved card token identifier (`"token_..."`).
    pub token_id: String,
    /// Marketplace split allocation rules (validated against `amount_minor`).
    pub splits: Vec<UnifiedSplitRule>,
    /// Optional caller-supplied UUID idempotency key (`given_id`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<Uuid>,
    /// Optional human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional callback URL for 3DS or redirect completion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_url: Option<String>,
    /// Optional validated merchant metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

impl UnifiedSplitPaymentRequest {
    /// Creates a new [`UnifiedSplitPaymentRequest`] for off-session split settlement.
    #[must_use]
    pub fn new(
        amount_minor: u64,
        currency: impl Into<String>,
        token_id: impl Into<String>,
        splits: Vec<UnifiedSplitRule>,
    ) -> Self {
        Self {
            amount_minor,
            currency: currency.into(),
            token_id: token_id.into(),
            splits,
            idempotency_key: None,
            description: None,
            callback_url: None,
            metadata: None,
        }
    }

    /// Attaches an explicit UUID idempotency key (`given_id`).
    #[must_use]
    pub const fn idempotency_key(mut self, idempotency_key: Uuid) -> Self {
        self.idempotency_key = Some(idempotency_key);
        self
    }

    /// Attaches an optional human-readable payment description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Attaches an optional redirect callback URL.
    #[must_use]
    pub fn callback_url(mut self, callback_url: impl Into<String>) -> Self {
        self.callback_url = Some(callback_url.into());
        self
    }

    /// Attaches validated key/value [`Metadata`].
    #[must_use]
    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Unified marketplace split payment response across payment gateways.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedSplitPaymentResponse {
    /// Gateway transaction/payment identifier.
    pub transaction_id: String,
    /// Normalized payment status string.
    pub status: String,
    /// Total payment amount in minor currency units (`u64`).
    pub amount_minor: u64,
    /// ISO 4217 currency code.
    pub currency: Currency,
    /// Number of split allocations attached to the payment.
    pub split_count: usize,
    /// Raw split allocation records returned by the gateway.
    pub splits: Vec<serde_json::Value>,
}

/// Companion `some-payment` orthogonal capability trait for marketplace split settlements.
///
/// # Examples
///
/// ```
/// use some_moyasar::prelude::{MoyasarClient, SecretKey};
/// use some_moyasar::unified::SplitSettlementCapability;
///
/// let key = SecretKey::new("sk_test_split_cap_demo").unwrap();
/// let client = MoyasarClient::new(key).unwrap();
/// fn assert_splits<S: SplitSettlementCapability>(_s: &S) {}
/// assert_splits(&client);
/// ```
pub trait SplitSettlementCapability: Send + Sync {
    /// Concrete gateway error type.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Creates a marketplace payment with atomic split allocations (`POST /v1/payments`).
    fn create_split_payment<'a>(
        &'a self,
        request: UnifiedSplitPaymentRequest,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedSplitPaymentResponse, Self::Error>> + Send + 'a>>;
}

impl SplitSettlementCapability for MoyasarClient<SecretKey> {
    type Error = MoyasarError;

    fn create_split_payment<'a>(
        &'a self,
        request: UnifiedSplitPaymentRequest,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedSplitPaymentResponse, Self::Error>> + Send + 'a>>
    {
        Box::pin(async move {
            let params = build_split_payment_params(request)?;
            let payment = self.payments().create(&params).await?;
            let splits = payment.splits.unwrap_or_default();
            Ok(UnifiedSplitPaymentResponse {
                transaction_id: payment.id,
                status: payment.status.as_str().to_owned(),
                amount_minor: payment.amount,
                currency: payment.currency,
                split_count: splits.len(),
                splits,
            })
        })
    }
}

fn build_split_payment_params(
    request: UnifiedSplitPaymentRequest,
) -> Result<CreatePaymentParams, MoyasarError> {
    let cur: Currency = request.currency.parse()?;
    let mut source = TokenSource::new(request.token_id)?;
    source.three_ds = Some(false);
    let splits: Vec<PaymentSplitRequest> = request
        .splits
        .into_iter()
        .map(UnifiedSplitRule::into_moyasar)
        .collect();

    let mut builder = CreatePaymentParams::builder()
        .amount(request.amount_minor)
        .currency(cur)
        .payment_source(PaymentSourceRequest::Token(source))
        .payment_splits(splits);

    if let Some(key) = request.idempotency_key {
        builder = builder.given_id(key);
    }
    if let Some(desc) = request.description {
        builder = builder.description(desc);
    }
    if let Some(cb) = request.callback_url {
        builder = builder.callback_url(cb);
    }
    if let Some(meta) = request.metadata {
        builder = builder.metadata(meta);
    }
    builder.build()
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn test_client(uri: &str) -> MoyasarClient<SecretKey> {
        MoyasarClient::builder(SecretKey::new("sk_test_split_adapter").unwrap())
            .base_url(uri)
            .build()
            .unwrap()
    }

    async fn mount_split_payment_mock(server: &MockServer, vendor_id: Uuid, platform_id: Uuid) {
        let now = Utc::now().to_rfc3339();
        Mock::given(method("POST"))
            .and(path("/v1/payments"))
            .and(body_partial_json(json!({
                "amount": 15000,
                "currency": "SAR",
                "splits": [
                    {
                        "amount": 13500,
                        "recipient_type": "beneficiary",
                        "recipient_id": vendor_id,
                        "reference": "order-1001-vendor",
                        "fee_source": false,
                        "refundable": true
                    },
                    {
                        "amount": 1500,
                        "recipient_type": "platform",
                        "recipient_id": platform_id,
                        "fee_source": true,
                        "refundable": true
                    }
                ]
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "pay_split_01",
                "status": "paid",
                "amount": 15000,
                "amount_format": "150.00 SAR",
                "fee": 250,
                "fee_format": "2.50 SAR",
                "refunded": 0,
                "refunded_format": "0.00 SAR",
                "captured": 15000,
                "captured_format": "150.00 SAR",
                "currency": "SAR",
                "source": { "type": "token" },
                "splits": [
                    { "id": "sp_1", "amount": 13500, "recipient_type": "beneficiary" },
                    { "id": "sp_2", "amount": 1500, "recipient_type": "platform" }
                ],
                "created_at": now,
                "updated_at": now
            })))
            .expect(1)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn create_split_payment_dispatches_validated_splits() {
        let server = MockServer::start().await;
        let vendor_id = Uuid::from_u128(0x1111);
        let platform_id = Uuid::from_u128(0x2222);
        mount_split_payment_mock(&server, vendor_id, platform_id).await;

        let client = test_client(&server.uri());
        let req = UnifiedSplitPaymentRequest::new(
            15_000,
            "SAR",
            "token_split_01",
            vec![
                UnifiedSplitRule::new(13_500, UnifiedSplitRecipientRole::Beneficiary, vendor_id)
                    .reference("order-1001-vendor")
                    .fee_source(false),
                UnifiedSplitRule::new(1_500, UnifiedSplitRecipientRole::Platform, platform_id)
                    .fee_source(true),
            ],
        )
        .idempotency_key(Uuid::new_v4())
        .description("Marketplace Order #1001");

        let resp = client.create_split_payment(req).await.unwrap();
        assert_eq!(resp.transaction_id, "pay_split_01");
        assert_eq!(resp.status, "paid");
        assert_eq!(resp.amount_minor, 15_000);
        assert_eq!(resp.currency, Currency::SAR);
        assert_eq!(resp.split_count, 2);
    }

    #[tokio::test]
    async fn create_split_payment_rejects_oversubscribed_and_duplicate_fee_source() {
        let server = MockServer::start().await;
        let client = test_client(&server.uri());
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);

        let oversubscribed = UnifiedSplitPaymentRequest::new(
            10_000,
            "SAR",
            "token_split_02",
            vec![
                UnifiedSplitRule::new(7_000, UnifiedSplitRecipientRole::Beneficiary, a),
                UnifiedSplitRule::new(5_000, UnifiedSplitRecipientRole::Platform, b),
            ],
        );
        assert!(matches!(
            client.create_split_payment(oversubscribed).await,
            Err(MoyasarError::SplitTotalExceedsAmount)
        ));

        let dup_fee = UnifiedSplitPaymentRequest::new(
            10_000,
            "SAR",
            "token_split_03",
            vec![
                UnifiedSplitRule::new(5_000, UnifiedSplitRecipientRole::Beneficiary, a)
                    .fee_source(true),
                UnifiedSplitRule::new(5_000, UnifiedSplitRecipientRole::Platform, b)
                    .fee_source(true),
            ],
        );
        assert!(matches!(
            client.create_split_payment(dup_fee).await,
            Err(MoyasarError::MultipleFeeSources)
        ));

        let zero_split = UnifiedSplitPaymentRequest::new(
            10_000,
            "SAR",
            "token_split_04",
            vec![UnifiedSplitRule::new(
                0,
                UnifiedSplitRecipientRole::Entity,
                a,
            )],
        );
        assert!(matches!(
            client.create_split_payment(zero_split).await,
            Err(MoyasarError::InvalidSplitAmount)
        ));
    }
}
