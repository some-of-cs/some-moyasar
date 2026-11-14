//! Public prelude re-exporting core microkernel and feature-enabled resource types.

#[cfg(feature = "env-keys")]
pub use crate::kernel::resolve_env_secret;

pub use crate::kernel::{
    API_VERSION, Amount, ApiErrorPayload, ApiErrorType, AuthKey, AuthMode, Currency,
    DEFAULT_BASE_URL, FieldError, GivenIdPolicy, METADATA_KEY_MAX_LEN, METADATA_MAX_KEYS,
    METADATA_VALUE_MAX_LEN, MOYASAR_SUBSCRIPTION_NAMESPACE, MOYASAR_VERSION_HEADER, Meta, Metadata,
    MoyasarClient, MoyasarClientBuilder, MoyasarClientConfig, MoyasarError, PaginatedList,
    Paginator, PublishableAuthKey, PublishableKey, RecipientType, ResponseCode, SecretAuthKey,
    SecretKey, build_default_headers, decode_json_response, derive_subscription_given_id,
    deserialize_field_errors, encode_basic_auth, execute_with_retry, inject_given_id,
    is_valid_id_char, is_valid_path_segment, is_valid_prefixed_id, send_get_json, send_with_retry,
    serialize_opt_secret, serialize_secret, validate_path_segment,
};

#[cfg(feature = "payments")]
pub use crate::resources::payments::{
    AfTSender, AftSender, ApplePaySource, CardBrand, CardSourceResponse, CreatePaymentParams,
    CreatePaymentParamsBuilder, CreditCardSource, IssuerCardType, ListParams,
    MADA_CAPTURE_WINDOW_DAYS, Payment, PaymentId, PaymentSourceRequest, PaymentSourceResponse,
    PaymentSplitRequest, PaymentSplitResponse, PaymentStatus, PaymentsService, ReversalPlan,
    ReversalStrategy, SADAD_BILLER_PRODUCTION_PRIMARY, SADAD_BILLER_PRODUCTION_SECONDARY,
    SADAD_BILLER_SANDBOX, SADAD_MAX_EXPIRY_DAYS, SANDBOX_SADAD_BILLER_CODE,
    SANDBOX_STCPAY_INSUFFICIENT_FUNDS, SANDBOX_STCPAY_REJECTED, SANDBOX_STCPAY_SUCCESS,
    SadadBillSource, SadadBillSourceResponse, SamsungPaySource, SplitRecipientType,
    SplitRefundAllocation, StcPayOtpPayment, StcPayOtpRequest, StcPaySource, StcPaySourceResponse,
    TokenSource, UpdatePaymentParams, VOID_WINDOW_SECS, VoidOrRefund, WalletSourceResponse,
    calculate_proportional_refunds, can_void_payment, effective_sadad_expiry, is_amex,
    is_mada_hold_likely_expired, is_otp_cooldown_error, is_within_void_window, luhn_valid,
    normalize_saudi_mobile, remaining_refundable_amount, validate_card_fields, validate_cvc,
    validate_expiry, validate_expiry_against, validate_refund_amount, validate_sadad_expiry,
    validate_sadad_expiry_with_invoice, validate_splits,
};

#[cfg(feature = "payment-intents")]
pub use crate::resources::payment_intents::{
    CreatePaymentIntentParams, FulfillIntentParams, FulfillmentOutcome, INTENT_EXPIRY_SECS,
    IntentId, IntentPayment, IntentPaymentStatus, PaymentIntent, PaymentIntentStatus,
    PaymentIntentsService, PaymentSourceRequest as FulfillPaymentSourceRequest,
    validate_client_secret, validate_pre_fulfillment,
};

#[cfg(feature = "tokens")]
pub use crate::resources::tokens::{
    CreateTokenParams, TOKEN_ID_PREFIX, Token, TokenId, TokenStatus, TokensService,
    can_transition_token_status, is_token_chargeable, is_token_deletable, require_active_token,
};

#[cfg(feature = "invoices")]
pub use crate::resources::invoices::{
    CreateInvoiceParams, Invoice, InvoiceStatus, InvoicesService, ListInvoicesParams,
    MAX_SADAD_EXPIRY_DAYS, UpdateInvoiceParams, sync_sadad_expiry, validate_invoice_cancelable,
};

#[cfg(feature = "card-auths")]
pub use crate::resources::card_auths::{
    CardAuthBridgeData, CardAuthId, CardAuthResult, CardAuthStatus, CardAuthentication,
    CardAuthsService, CreateCardAuthParams, ExternalMpiData, MpiParameters, extract_mpi_from_auth,
    extract_mpi_result,
};

#[cfg(feature = "settlements-ledger")]
pub use crate::resources::settlements::{
    InternalTransaction, InternalTransactionId, InternalTransactionsService,
    ListInternalTransactionsParams, ListSettlementLinesParams, ListSettlementsParams,
    ListTransferLinesParams, ListTransfersParams, MerchantBalance, Settlement, SettlementId,
    SettlementLine, SettlementRecipientType, SettlementsService, Transfer, TransferId,
    TransferLine, TransfersService,
};

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
pub use crate::resources::sources::{
    ApplePayMerchantSession, ApplePayService, BinLookupParams, InitiateApplePayParams,
    IssuerCardType as SourceIssuerCardType, IssuerMetadata, SourcesService,
};

#[cfg(feature = "webhooks-api")]
pub use crate::resources::webhooks_api::{
    CreateWebhookParams, ListWebhookAttemptsParams, WebhookAttempt, WebhookEndpoint, WebhookSecret,
    WebhooksService, validate_https_url,
};

#[cfg(feature = "webhook-engine")]
pub use crate::webhook_engine::{
    ConstantTimeSecretVerifier, DOMAIN_SEPARATION_KEY, InMemoryDeduplicator, MAX_FUTURE_SKEW,
    NoopDeduplicator, REPLAY_TOLERANCE, WebhookDeduplicator, WebhookError, WebhookEvent,
    WebhookEventType, validate_timestamp_at,
};

#[cfg(feature = "stripe-compat")]
pub use crate::stripe_compat::{
    CustomerChargeParams, CustomerVault, CustomersService, InMemoryCustomerVault, PlanInterval,
    RefundCreateParams, RefundReceipt, RefundRoute, RefundStrategy, RefundsService, ReversalAction,
    Subscription, SubscriptionCycle, SubscriptionCycleId, SubscriptionCycleRefundParams,
    SubscriptionCycleRefundResult, SubscriptionPlan, SubscriptionsService,
    inject_customer_metadata, next_period_bounds, pro_rata_refund, select_refund_route,
};

#[cfg(feature = "unified-gateway")]
pub use crate::unified::{
    UnifiedChargeResponse, UnifiedIntentResponse, UnifiedPaymentGateway, UnifiedReversalResponse,
    UnifiedWebhookEvent,
};

#[cfg(any(test, feature = "test-utils"))]
pub use crate::test_utils::{
    ALL_WEBHOOK_EVENT_STRINGS, MoyasarMockServer, SADAD_SANDBOX_BILLER,
    STCPAY_SANDBOX_INSUFFICIENT_FUNDS, STCPAY_SANDBOX_REJECTED, STCPAY_SANDBOX_SUCCESS,
    all_webhook_event_fixtures, iso8583_error_fixture, sadad_sandbox_fixture,
    stcpay_sandbox_fixture, webhook_event_fixture,
};
