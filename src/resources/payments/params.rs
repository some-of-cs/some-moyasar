//! Request parameter builder for `POST /v1/payments` (US-2.1.0 Unit 2).

use core::fmt;

use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::metadata::Metadata;
use crate::kernel::types::Currency;

use super::aft::AftSender;
use super::source::PaymentSourceRequest;
use super::split_validation::validate_splits;
use super::splits::PaymentSplitRequest;

fn invalid_req_err(message: &str) -> MoyasarError {
    MoyasarError::Api {
        error_type: ApiErrorType::InvalidRequestError,
        message: message.to_owned(),
        errors: Vec::new(),
        response_code: None,
    }
}

/// Validated request payload for creating a payment via `POST /v1/payments`.
///
/// Construct using [`CreatePaymentParams::builder()`].
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct CreatePaymentParams {
    /// Payment amount in the currency's smallest minor unit (e.g., Halalas for SAR).
    pub amount: u64,
    /// ISO 4217 currency code.
    pub currency: Currency,
    /// Payment source object (e.g., token, creditcard, Apple Pay, STC Pay, SADAD).
    pub source: Value,
    /// Optional human-readable payment description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional 3DS / redirect callback URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_url: Option<String>,
    /// Optional client-supplied UUIDv4/v5 idempotency key (`given_id`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub given_id: Option<Uuid>,
    /// Optional validated merchant metadata map.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    /// Optional marketplace split allocations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub splits: Option<Vec<Value>>,
    /// Optional Account Funding Transaction (AFT) sender details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender: Option<Value>,
}

impl fmt::Debug for CreatePaymentParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreatePaymentParams")
            .field("amount", &self.amount)
            .field("currency", &self.currency)
            .field("source", &"[REDACTED]")
            .field("given_id", &self.given_id)
            .field("metadata", &self.metadata)
            .finish()
    }
}

impl CreatePaymentParams {
    /// Starts a new [`CreatePaymentParamsBuilder`].
    pub fn builder() -> CreatePaymentParamsBuilder {
        CreatePaymentParamsBuilder::default()
    }
}

/// Fluent builder for [`CreatePaymentParams`].
#[derive(Clone, Default)]
pub struct CreatePaymentParamsBuilder {
    amount: Option<u64>,
    currency: Option<Currency>,
    source: Option<Value>,
    typed_source: Option<PaymentSourceRequest>,
    description: Option<String>,
    callback_url: Option<String>,
    given_id: Option<Uuid>,
    metadata: Option<Metadata>,
    splits: Option<Vec<Value>>,
    typed_splits: Option<Vec<PaymentSplitRequest>>,
    sender: Option<Value>,
}

impl fmt::Debug for CreatePaymentParamsBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreatePaymentParamsBuilder")
            .field("amount", &self.amount)
            .field("currency", &self.currency)
            .field(
                "has_source",
                &(self.source.is_some() || self.typed_source.is_some()),
            )
            .field("given_id", &self.given_id)
            .finish()
    }
}

impl CreatePaymentParamsBuilder {
    /// Sets the payment amount in minor units (must be strictly positive).
    pub fn amount(mut self, amount: u64) -> Self {
        self.amount = Some(amount);
        self
    }

    /// Sets the ISO 4217 currency code.
    pub fn currency(mut self, currency: Currency) -> Self {
        self.currency = Some(currency);
        self
    }

    /// Sets the raw JSON payment source payload.
    pub fn source(mut self, source: Value) -> Self {
        self.source = Some(source);
        self.typed_source = None;
        self
    }

    /// Sets a strongly typed [`PaymentSourceRequest`], validated during [`Self::build`].
    pub fn payment_source(mut self, source: impl Into<PaymentSourceRequest>) -> Self {
        self.typed_source = Some(source.into());
        self.source = None;
        self
    }

    /// Sets an optional merchant description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets an optional 3DS / redirect callback URL.
    pub fn callback_url(mut self, callback_url: impl Into<String>) -> Self {
        self.callback_url = Some(callback_url.into());
        self
    }

    /// Sets an explicit idempotency UUID (`given_id`).
    pub fn given_id(mut self, given_id: Uuid) -> Self {
        self.given_id = Some(given_id);
        self
    }

    /// Sets validated merchant metadata.
    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Sets optional raw JSON marketplace split allocations.
    pub fn splits(mut self, splits: Vec<Value>) -> Self {
        self.splits = Some(splits);
        self.typed_splits = None;
        self
    }

    /// Sets typed marketplace split allocations, validated against `amount` during [`Self::build`].
    pub fn payment_splits(mut self, splits: Vec<PaymentSplitRequest>) -> Self {
        self.typed_splits = Some(splits);
        self.splits = None;
        self
    }

    /// Sets optional raw JSON Account Funding Transaction (AFT) sender details.
    pub fn sender(mut self, sender: Value) -> Self {
        self.sender = Some(sender);
        self
    }

    /// Sets strongly typed [`AftSender`] details for Account Funding Transactions.
    pub fn aft_sender(mut self, sender: AftSender) -> Self {
        self.sender = serde_json::to_value(sender).ok();
        self
    }

    /// Validates required fields and builds [`CreatePaymentParams`].
    pub fn build(self) -> Result<CreatePaymentParams, MoyasarError> {
        let amount = self
            .amount
            .filter(|&a| a > 0)
            .ok_or_else(|| invalid_req_err("amount must be strictly positive"))?;
        let currency = self
            .currency
            .ok_or_else(|| invalid_req_err("currency is required"))?;
        let source = resolve_source(self.typed_source, self.source)?;
        let splits = resolve_splits(self.typed_splits, self.splits, amount)?;

        Ok(CreatePaymentParams {
            amount,
            currency,
            source,
            description: self.description,
            callback_url: self.callback_url,
            given_id: self.given_id,
            metadata: self.metadata,
            splits,
            sender: self.sender,
        })
    }
}

fn resolve_source(
    typed_source: Option<PaymentSourceRequest>,
    raw_source: Option<Value>,
) -> Result<Value, MoyasarError> {
    if let Some(typed) = typed_source {
        typed.validate()?;
        return serde_json::to_value(typed).map_err(|e| invalid_req_err(&e.to_string()));
    }
    raw_source.ok_or_else(|| invalid_req_err("source is required"))
}

fn resolve_splits(
    typed_splits: Option<Vec<PaymentSplitRequest>>,
    raw_splits: Option<Vec<Value>>,
    amount: u64,
) -> Result<Option<Vec<Value>>, MoyasarError> {
    if let Some(typed) = typed_splits {
        validate_splits(&typed, amount)?;
        let values = typed
            .into_iter()
            .map(|s| serde_json::to_value(s).map_err(|e| invalid_req_err(&e.to_string())))
            .collect::<Result<Vec<Value>, MoyasarError>>()?;
        return Ok(Some(values));
    }
    Ok(raw_splits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::payments::source_token::TokenSource;
    use crate::resources::payments::splits::SplitRecipientType;
    use serde_json::json;

    #[test]
    fn builder_serializes_required_and_optional_fields() {
        let mut meta = Metadata::new();
        meta.insert("order_id", "ORD-1").expect("valid metadata");
        let gid = Uuid::parse_str("3fa85f64-5717-4562-b3fc-2c963f66afa6").expect("valid uuid");

        let params = CreatePaymentParams::builder()
            .amount(15000)
            .currency(Currency::SAR)
            .source(json!({ "type": "token", "token": "token_123" }))
            .description("Order #1")
            .callback_url("https://merchant.example/cb")
            .given_id(gid)
            .metadata(meta)
            .build()
            .expect("valid params");

        let serialized = serde_json::to_value(&params).expect("serializes");
        assert_eq!(serialized["amount"], 15000);
        assert_eq!(serialized["currency"], "SAR");
        assert_eq!(serialized["source"]["type"], "token");
        assert_eq!(serialized["description"], "Order #1");
        assert_eq!(serialized["callback_url"], "https://merchant.example/cb");
        assert_eq!(serialized["given_id"], gid.to_string());
        assert_eq!(serialized["metadata"]["order_id"], "ORD-1");
        assert!(serialized.get("splits").is_none());
        assert!(serialized.get("sender").is_none());
    }

    #[test]
    fn builder_rejects_missing_or_zero_amount() {
        let missing_amount = CreatePaymentParams::builder()
            .currency(Currency::SAR)
            .source(json!({ "type": "token", "token": "token_123" }))
            .build();
        assert!(matches!(
            missing_amount,
            Err(MoyasarError::Api {
                error_type: ApiErrorType::InvalidRequestError,
                ..
            })
        ));

        let zero_amount = CreatePaymentParams::builder()
            .amount(0)
            .currency(Currency::SAR)
            .source(json!({ "type": "token", "token": "token_123" }))
            .build();
        assert!(zero_amount.is_err());
    }

    #[test]
    fn typed_payment_source_splits_and_aft_sender_validate_in_build() {
        let token = TokenSource::new("token_valid_123").expect("valid token");
        let valid_splits = vec![
            PaymentSplitRequest::new(8_500, SplitRecipientType::Beneficiary, Uuid::nil())
                .with_fee_source(false),
            PaymentSplitRequest::new(1_500, SplitRecipientType::Platform, Uuid::nil())
                .with_fee_source(true),
        ];
        let sender = AftSender::new("Maher", "Mansour")
            .with_nationality("SA")
            .with_id("national_id", "1020304050");

        let params = CreatePaymentParams::builder()
            .amount(10_000)
            .currency(Currency::SAR)
            .payment_source(token.clone())
            .payment_splits(valid_splits)
            .aft_sender(sender)
            .build()
            .expect("valid typed params");

        let serialized = serde_json::to_value(&params).expect("serializes");
        assert_eq!(serialized["source"]["type"], "token");
        assert_eq!(serialized["splits"].as_array().expect("splits").len(), 2);
        assert_eq!(serialized["sender"]["first_name"], "Maher");

        let over_splits = vec![
            PaymentSplitRequest::new(7_000, SplitRecipientType::Beneficiary, Uuid::nil()),
            PaymentSplitRequest::new(5_000, SplitRecipientType::Platform, Uuid::nil()),
        ];
        let over_err = CreatePaymentParams::builder()
            .amount(10_000)
            .currency(Currency::SAR)
            .payment_source(token)
            .payment_splits(over_splits)
            .build()
            .expect_err("must reject splits exceeding amount");
        assert!(matches!(over_err, MoyasarError::SplitTotalExceedsAmount));
    }
}
