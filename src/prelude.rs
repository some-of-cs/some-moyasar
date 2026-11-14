//! Public prelude re-exporting core microkernel and feature-enabled resource types.

pub use crate::kernel::auth::{
    AuthKey, AuthMode, PublishableAuthKey, PublishableKey, SecretAuthKey, SecretKey,
};
pub use crate::kernel::client::{MoyasarClient, MoyasarClientBuilder, MoyasarClientConfig};
pub use crate::kernel::error::{ApiErrorPayload, ApiErrorType, MoyasarError};
pub use crate::kernel::idempotency::{GivenIdPolicy, inject_given_id};
pub use crate::kernel::idempotency_v5::{
    MOYASAR_SUBSCRIPTION_NAMESPACE, derive_subscription_given_id,
};
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

#[cfg(feature = "settlements-ledger")]
pub use crate::resources::settlements::balance::MerchantBalance;
#[cfg(feature = "settlements-ledger")]
pub use crate::resources::settlements::internal_transactions::ListInternalTransactionsParams;
#[cfg(feature = "settlements-ledger")]
pub use crate::resources::settlements::internal_txn_models::{
    InternalTransaction, InternalTransactionId,
};
#[cfg(feature = "settlements-ledger")]
pub use crate::resources::settlements::lines::{ListSettlementLinesParams, SettlementLine};
#[cfg(feature = "settlements-ledger")]
pub use crate::resources::settlements::list::ListSettlementsParams;
#[cfg(feature = "settlements-ledger")]
pub use crate::resources::settlements::models::{
    Settlement, SettlementId, SettlementRecipientType,
};
#[cfg(feature = "settlements-ledger")]
pub use crate::resources::settlements::service::{
    InternalTransactionsService, SettlementsService, TransfersService,
};
#[cfg(feature = "settlements-ledger")]
pub use crate::resources::settlements::transfer_models::{Transfer, TransferId, TransferLine};
#[cfg(feature = "settlements-ledger")]
pub use crate::resources::settlements::transfers::{ListTransferLinesParams, ListTransfersParams};

#[cfg(feature = "payouts")]
pub use crate::resources::payouts::{
    BankDestination, BulkPayoutEntry, BulkPayoutResponse, CreateBulkPayoutParams,
    CreatePayoutAccountParams, CreatePayoutParams, KNOWN_PURPOSES, ListPayoutAccountsParams,
    ListPayoutsParams, PAYOUT_SEQUENCE_NUMBER_LEN, ParsePayoutPurposeError, Payout, PayoutAccount,
    PayoutAccountId, PayoutAccountProperties, PayoutAccountType, PayoutAccountsService,
    PayoutChannel, PayoutDestination, PayoutId, PayoutPurpose, PayoutStatus, PayoutsService,
    WalletDestination, validate_sa_iban, validate_sa_mobile, validate_sequence_number,
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
pub use crate::resources::webhooks_api::models::{WebhookAttempt, WebhookEndpoint, WebhookSecret};

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

#[cfg(feature = "stripe-compat")]
pub use crate::stripe_compat::customers::{
    CustomerChargeParams, CustomerVault, CustomersService, inject_customer_metadata,
};
#[cfg(feature = "stripe-compat")]
pub use crate::stripe_compat::in_memory_vault::InMemoryCustomerVault;
#[cfg(feature = "stripe-compat")]
pub use crate::stripe_compat::pro_rata::pro_rata_refund;
#[cfg(feature = "stripe-compat")]
pub use crate::stripe_compat::refund_strategy::{RefundRoute, select_refund_route};
#[cfg(feature = "stripe-compat")]
pub use crate::stripe_compat::refunds::{RefundCreateParams, RefundReceipt, RefundsService};
#[cfg(feature = "stripe-compat")]
pub use crate::stripe_compat::subscription_plan::{
    PlanInterval, SubscriptionPlan, next_period_bounds,
};
#[cfg(feature = "stripe-compat")]
pub use crate::stripe_compat::subscriptions_billing::{
    SubscriptionCycleChargeParams, SubscriptionsService,
};
#[cfg(feature = "stripe-compat")]
pub use crate::stripe_compat::subscriptions_model::{
    RefundStrategy, ReversalAction, Subscription, SubscriptionCycle, SubscriptionCycleId,
    SubscriptionCycleRefundParams, SubscriptionCycleRefundResult,
};

#[cfg(feature = "unified-gateway")]
pub use crate::unified::{
    PaymentCapability, PayoutCapability, PayoutEngine, UnifiedChargeResponse, UnifiedIntentRequest,
    UnifiedIntentResponse, UnifiedPaymentGateway, UnifiedReversalResponse,
    UnifiedTokenChargeRequest, UnifiedWebhookEvent, WebhookCapability, WebhookVerifier,
};

#[cfg(any(test, feature = "test-utils"))]
pub use crate::test_utils::fixtures::*;
#[cfg(any(test, feature = "test-utils"))]
pub use crate::test_utils::mock::MoyasarMockServer;
