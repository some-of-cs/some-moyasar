//! Optional `some-payment-core::PaymentProvider` adapter for Moyasar.

mod config;
mod mapper;
/// Core [`MoyasarProvider`](moyasar_provider::MoyasarProvider) adapter implementation.
pub mod moyasar_provider;

#[cfg(test)]
mod conformance_tests;
