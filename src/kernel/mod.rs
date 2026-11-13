//! Core microkernel and transport layer for `some-moyasar`.

#[cfg(feature = "us_1_1_0")]
pub mod auth;
#[cfg(feature = "us_1_1_0")]
pub mod client;
#[cfg(feature = "us_1_1_0")]
pub mod transport;

#[cfg(feature = "us_1_2_0")]
pub mod idempotency;
#[cfg(feature = "us_1_2_0")]
pub mod idempotency_v5;
#[cfg(feature = "us_1_2_0")]
pub mod metadata;
#[cfg(feature = "us_1_2_0")]
pub mod pagination;
#[cfg(feature = "us_1_2_0")]
pub mod paginator_stream;

#[cfg(feature = "us_1_3_0")]
pub mod error;
#[cfg(feature = "us_1_3_0")]
pub mod response_code;
#[cfg(feature = "us_1_3_0")]
pub mod types;
