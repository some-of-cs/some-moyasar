//! # some-moyasar
//!
//! Idiomatic, microkernel-architected Rust SDK for the Moyasar Payment Gateway.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[cfg(feature = "core")]
pub mod kernel;

#[cfg(feature = "core")]
pub mod prelude;

#[cfg(feature = "core")]
pub mod resources;
