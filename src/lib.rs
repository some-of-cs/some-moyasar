//! # some-moyasar
//!
//! Idiomatic, microkernel-architected Rust SDK for the Moyasar Payment Gateway.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[cfg(feature = "core")]
pub mod kernel;

#[cfg(feature = "core")]
pub mod prelude;

#[cfg(any(
    feature = "payment-intents",
    feature = "tokens",
    feature = "invoices",
    feature = "card-auths"
))]
pub mod resources;
