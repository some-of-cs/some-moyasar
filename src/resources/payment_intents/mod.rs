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
