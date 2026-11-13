//! Request parameter builder for `POST /v1/payments` (US-2.1.0 Unit 2).

use core::fmt;

use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::kernel::error::{ApiErrorType, MoyasarError};
use crate::kernel::metadata::Metadata;
use crate::kernel::types::Currency;

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
    description: Option<String>,
    callback_url: Option<String>,
    given_id: Option<Uuid>,
    metadata: Option<Metadata>,
    splits: Option<Vec<Value>>,
    sender: Option<Value>,
}

impl fmt::Debug for CreatePaymentParamsBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreatePaymentParamsBuilder")
            .field("amount", &self.amount)
            .field("currency", &self.currency)
            .field("has_source", &self.source.is_some())
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

    /// Sets the payment source payload.
    pub fn source(mut self, source: Value) -> Self {
        self.source = Some(source);
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

    /// Sets optional marketplace split allocations.
    pub fn splits(mut self, splits: Vec<Value>) -> Self {
        self.splits = Some(splits);
        self
    }

    /// Sets optional Account Funding Transaction (AFT) sender details.
    pub fn sender(mut self, sender: Value) -> Self {
        self.sender = Some(sender);
        self
    }

    /// Validates required fields and builds [`CreatePaymentParams`].
    pub fn build(self) -> Result<CreatePaymentParams, MoyasarError> {
        let amount = self.amount.filter(|&a| a > 0).ok_or_else(|| MoyasarError::Api {
            error_type: ApiErrorType::InvalidRequestError,
            message: "amount must be strictly positive".to_owned(),
            errors: Vec::new(),
            response_code: None,
        })?;
        let currency = self.currency.ok_or_else(|| MoyasarError::Api {
            error_type: ApiErrorType::InvalidRequestError,
            message: "currency is required".to_owned(),
            errors: Vec::new(),
            response_code: None,
        })?;
        let source = self.source.ok_or_else(|| MoyasarError::Api {
            error_type: ApiErrorType::InvalidRequestError,
            message: "source is required".to_owned(),
            errors: Vec::new(),
            response_code: None,
        })?;

        Ok(CreatePaymentParams {
            amount,
            currency,
            source,
            description: self.description,
            callback_url: self.callback_url,
            given_id: self.given_id,
            metadata: self.metadata,
            splits: self.splits,
            sender: self.sender,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
