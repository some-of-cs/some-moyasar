//! Moyasar Payments API resource (`v1/payments`).
//!
//! Provides creation, retrieval, listing, lifecycle operations (update, capture,
//! void, refund, smart void-or-refund), polymorphic payment sources (card,
//! Apple Pay, Samsung Pay, saved tokens, STC Pay, SADAD), and marketplace
//! splits / Account Funding Transactions (AFT).

pub mod aft;
pub mod capture;
pub mod card_validation;
pub mod create;
pub mod list;
pub mod models;
pub mod params;
pub mod refund;
pub mod retrieve;
pub mod sadad_expiry;
pub mod service;
pub mod smart_void;
pub mod source;
pub mod source_card;
pub mod source_digital_wallet;
pub mod source_response;
pub mod source_sadad;
pub mod source_stcpay;
pub mod source_token;
pub mod split_validation;
pub mod splits;
pub mod splits_response;
pub mod stcpay_otp;
pub mod update;
pub mod void;

pub use aft::{AfTSender, AftSender};
pub use capture::{MADA_CAPTURE_WINDOW_DAYS, is_mada_hold_likely_expired};
pub use card_validation::{
    is_amex, luhn_valid, validate_card_fields, validate_cvc, validate_expiry,
    validate_expiry_against,
};
pub use list::ListParams;
pub use models::{Payment, PaymentStatus};
pub use params::{CreatePaymentParams, CreatePaymentParamsBuilder};
pub use refund::{remaining_refundable_amount, validate_refund_amount};
pub use retrieve::PaymentId;
pub use sadad_expiry::{
    SADAD_MAX_EXPIRY_DAYS, SANDBOX_SADAD_BILLER_CODE, SANDBOX_STCPAY_INSUFFICIENT_FUNDS,
    SANDBOX_STCPAY_REJECTED, SANDBOX_STCPAY_SUCCESS, effective_sadad_expiry, validate_sadad_expiry,
    validate_sadad_expiry_with_invoice,
};
pub use service::PaymentsService;
pub use smart_void::{ReversalPlan, ReversalStrategy, VoidOrRefund};
pub use source::{PaymentSourceRequest, PaymentSourceResponse};
pub use source_card::CreditCardSource;
pub use source_digital_wallet::{ApplePaySource, SamsungPaySource};
pub use source_response::{CardBrand, CardSourceResponse, IssuerCardType, WalletSourceResponse};
pub use source_sadad::{
    SADAD_BILLER_PRODUCTION_PRIMARY, SADAD_BILLER_PRODUCTION_SECONDARY, SADAD_BILLER_SANDBOX,
    SadadBillSource, SadadBillSourceResponse,
};
pub use source_stcpay::{StcPaySource, StcPaySourceResponse, normalize_saudi_mobile};
pub use source_token::TokenSource;
pub use split_validation::validate_splits;
pub use splits::{PaymentSplitRequest, SplitRecipientType};
pub use splits_response::{
    PaymentSplitResponse, SplitRefundAllocation, calculate_proportional_refunds,
};
pub use stcpay_otp::{StcPayOtpPayment, StcPayOtpRequest, is_otp_cooldown_error};
pub use update::UpdatePaymentParams;
pub use void::{
    VOID_WINDOW_SECS, can_void_payment, is_void_fallback_eligible, is_within_void_window,
};
