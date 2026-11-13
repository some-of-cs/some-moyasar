//! Moyasar Payment Intents resource (`/v1/payment_intents`).
//!
//! Supports server-side creation, retrieval, and cancellation with a `SecretKey`,
//! and browser/mobile fulfillment with a `PublishableKey` and `client_secret`.

pub mod create;
pub mod expiry;
pub mod fulfill;
pub mod fulfill_guard;
pub mod fulfill_params;
pub mod lifecycle;
pub mod models;
pub mod retrieve;
pub mod service;

pub use create::CreatePaymentIntentParams;
pub use fulfill::{IntentPayment, IntentPaymentStatus};
pub use fulfill_guard::{validate_client_secret, validate_pre_fulfillment};
pub use fulfill_params::{FulfillIntentParams, PaymentSourceRequest};
pub use lifecycle::FulfillmentOutcome;
pub use models::{INTENT_EXPIRY_SECS, IntentId, PaymentIntent, PaymentIntentStatus};
pub use service::PaymentIntentsService;
