//! Public prelude re-exporting core microkernel and feature-enabled resource types.

pub use crate::kernel::auth::{
    AuthKey, AuthMode, PublishableAuthKey, PublishableKey, SecretAuthKey, SecretKey,
};
pub use crate::kernel::client::{MoyasarClient, MoyasarClientBuilder, MoyasarClientConfig};
pub use crate::kernel::error::{ApiErrorPayload, ApiErrorType, MoyasarError};
pub use crate::kernel::idempotency::{GivenIdPolicy, inject_given_id};
pub use crate::kernel::idempotency_v5::derive_subscription_given_id;
pub use crate::kernel::metadata::Metadata;
pub use crate::kernel::pagination::{Meta, PaginatedList};
pub use crate::kernel::paginator_stream::Paginator;
pub use crate::kernel::response_code::{FieldError, ResponseCode, deserialize_field_errors};
pub use crate::kernel::transport::{execute_with_retry, send_with_retry};
pub use crate::kernel::types::{Amount, Currency, RecipientType};

#[cfg(feature = "payments")]
pub use crate::resources::payments::{
    AfTSender, AftSender, ApplePaySource, CardBrand, CardSourceResponse, CreatePaymentParams,
    CreatePaymentParamsBuilder, CreditCardSource, IssuerCardType, ListParams, Payment, PaymentId,
    PaymentSourceRequest, PaymentSourceResponse, PaymentSplitRequest, PaymentSplitResponse,
    PaymentStatus, PaymentsService, ReversalPlan, ReversalStrategy, SadadBillSource,
    SadadBillSourceResponse, SamsungPaySource, SplitRecipientType, SplitRefundAllocation,
    StcPayOtpPayment, StcPayOtpRequest, StcPaySource, StcPaySourceResponse, TokenSource,
    UpdatePaymentParams, VoidOrRefund, WalletSourceResponse,
};

#[cfg(feature = "payment-intents")]
pub use crate::resources::payment_intents::{
    CreatePaymentIntentParams, FulfillIntentParams, FulfillmentOutcome, INTENT_EXPIRY_SECS,
    IntentId, IntentPayment, IntentPaymentStatus, PaymentIntent, PaymentIntentStatus,
    PaymentIntentsService, PaymentSourceRequest as FulfillPaymentSourceRequest,
    validate_client_secret, validate_pre_fulfillment,
};

#[cfg(feature = "tokens")]
pub use crate::resources::tokens::{CreateTokenParams, Token, TokenId, TokenStatus, TokensService};

#[cfg(feature = "invoices")]
pub use crate::resources::invoices::{
    CreateInvoiceParams, Invoice, InvoiceStatus, InvoicesService, ListInvoicesParams,
    UpdateInvoiceParams, sync_sadad_expiry, validate_invoice_cancelable,
};

#[cfg(feature = "card-auths")]
pub use crate::resources::card_auths::{
    CardAuthBridgeData, CardAuthId, CardAuthResult, CardAuthStatus, CardAuthentication,
    CardAuthsService, CreateCardAuthParams, ExternalMpiData, MpiParameters,
};

#[cfg(feature = "sources")]
pub use crate::resources::sources::apple_pay::{ApplePayMerchantSession, InitiateApplePayParams};
#[cfg(feature = "sources")]
pub use crate::resources::sources::issuer::{
    BinLookupParams, IssuerCardType as SourceIssuerCardType, IssuerMetadata,
};
#[cfg(feature = "sources")]
pub use crate::resources::sources::service::{ApplePayService, SourcesService};

#[cfg(feature = "webhooks-api")]
pub use crate::resources::webhooks_api::WebhooksService;
#[cfg(feature = "webhooks-api")]
pub use crate::resources::webhooks_api::attempts::ListWebhookAttemptsParams;
#[cfg(feature = "webhooks-api")]
pub use crate::resources::webhooks_api::crud::{CreateWebhookParams, validate_https_url};
#[cfg(feature = "webhooks-api")]
pub use crate::resources::webhooks_api::models::{
    WebhookAttempt, WebhookEndpoint, WebhookSecret,
};

#[cfg(feature = "webhook-engine")]
pub use crate::webhook_engine::deduplication::{
    InMemoryDeduplicator, NoopDeduplicator, WebhookDeduplicator,
};
#[cfg(feature = "webhook-engine")]
pub use crate::webhook_engine::event_types::{WebhookError, WebhookEvent, WebhookEventType};
#[cfg(feature = "webhook-engine")]
pub use crate::webhook_engine::timestamp::{
    MAX_FUTURE_SKEW, REPLAY_TOLERANCE, validate_timestamp_at,
};
#[cfg(feature = "webhook-engine")]
pub use crate::webhook_engine::verifier::{ConstantTimeSecretVerifier, DOMAIN_SEPARATION_KEY};
