//! Core microkernel and transport layer for `some-moyasar`.

pub mod auth;
pub mod client;
pub mod transport;

pub mod idempotency;
pub mod idempotency_v5;
pub mod metadata;
pub mod pagination;
pub mod paginator_stream;

pub mod error;
pub mod response_code;
pub mod types;
