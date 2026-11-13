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
