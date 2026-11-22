//! Optional `some-payment-core::PaymentProvider` adapter for Moyasar.

mod config;
mod mapper;
pub mod moyasar_provider;

#[cfg(test)]
mod conformance_tests;
