//! # some-moyasar (`v0.8.0`)
//!
//! Idiomatic, microkernel-architected Rust SDK and constant-time webhook verification engine
//! for the [Moyasar Payment Gateway](https://docs.moyasar.com/api/api-introduction).
//!
//! ## Microkernel Architecture & Compile-Time Key Isolation
//!
//! `some-moyasar` is designed around a zero-unsafe (`#![forbid(unsafe_code)]`), feature-gated
//! microkernel (`core`) with compile-time typestate authentication key isolation:
//!
//! - **[`SecretKey`](crate::kernel::SecretKey) (`sk_test_...` / `sk_live_...`)**: Authenticates
//!   server-to-server operations via [`MoyasarClient<SecretKey>`](crate::kernel::MoyasarClient).
//!   Secrets are stored inside [`secrecy::SecretString`] with automatic `ZeroizeOnDrop` memory
//!   wiping and `[REDACTED]` [`Debug`](core::fmt::Debug) formatting.
//! - **[`PublishableKey`](crate::kernel::PublishableKey) (`pk_test_...` / `pk_live_...`)**:
//!   Authenticates client-originated endpoints (such as Apple Pay Web merchant session initiation
//!   and card tokenization) via [`MoyasarClient<PublishableKey>`](crate::kernel::MoyasarClient).
//!   Calling a secret-key-only service (e.g., `payouts()`, `settlements()`, or retrieving a
//!   `PaymentIntent` containing a `client_secret`) on a `PublishableKey` client is rejected at
//!   compile time.
//! - **Deterministic Body-Injected Idempotency (`given_id`)**: Automatically injects UUIDv4/UUIDv5
//!   `given_id` tokens into mutating JSON request payloads so transient 5xx and network timeouts
//!   can be retried safely without double-charging customers.
//!
//! ## Complete 18-Service Accessor & Engine Matrix (Modules 1–8, `v0.1.0`–`v0.8.0`)
//!
//! | # | Module & Version | Accessor / Entrypoint | Service / Engine Type | Feature Flag | Key Requirement |
//! | :- | :--- | :--- | :--- | :--- | :--- |
//! | 1 | **M1 (`v0.1.0`) Core Kernel** | `MoyasarClient::new` / `builder` | [`MoyasarClient<K>`](crate::kernel::MoyasarClient) | `core` | `SecretKey` or `PublishableKey` |
//! | 2 | **M2 (`v0.2.0`) Payments** | `client.payments()` | `PaymentsService` | `payments` | `SecretKey` (create supports `K: AuthKey`) |
//! | 3 | **M3 (`v0.3.0`) Payment Intents** | `client.payment_intents()` | `PaymentIntentsService` | `payment-intents` | `SecretKey` (fulfill supports `K: AuthKey`) |
//! | 4 | **M4 (`v0.4.0`) Tokens** | `client.tokens()` | `TokensService` | `tokens` | `SecretKey` (create supports `K: AuthKey`) |
//! | 5 | **M4 (`v0.4.0`) Invoices** | `client.invoices()` | `InvoicesService` | `invoices` | `SecretKey` |
//! | 6 | **M4 (`v0.4.0`) Standalone 3DS** | `client.card_auths()` | `CardAuthsService` | `card-auths` | `SecretKey` / `PublishableKey` |
//! | 7 | **M5 (`v0.5.0`) Payout Accounts** | `client.payout_accounts()` | `PayoutAccountsService` | `payouts` | `SecretKey` |
//! | 8 | **M5 (`v0.5.0`) Payouts** | `client.payouts()` | `PayoutsService` | `payouts` | `SecretKey` |
//! | 9 | **M6 (`v0.6.0`) Settlements** | `client.settlements()` | `SettlementsService` | `settlements-ledger` | `SecretKey` |
//! | 10 | **M6 (`v0.6.0`) Transfers** | `client.transfers()` | `TransfersService` | `settlements-ledger` | `SecretKey` |
//! | 11 | **M6 (`v0.6.0`) Ledger** | `client.internal_transactions()` | `InternalTransactionsService` | `settlements-ledger` | `SecretKey` |
//! | 12 | **M7 (`v0.7.0`) Sources (BIN/IIN)** | `client.sources()` | `SourcesService` | `sources` | `K: AuthKey` |
//! | 13 | **M7 (`v0.7.0`) Apple Pay** | `client.apple_pay()` | `ApplePayService` | `sources` | `PublishableKey` |
//! | 14 | **M7 (`v0.7.0`) Webhooks API** | `client.webhooks()` | `WebhooksService` | `webhooks-api` | `SecretKey` |
//! | 15 | **M7 (`v0.7.0`) Webhook Engine** | `ConstantTimeSecretVerifier::new` | `ConstantTimeSecretVerifier` | `webhook-engine` | Shared webhook secret |
//! | 16 | **M8 (`v0.8.0`) Stripe Refunds** | `client.refunds()` | `RefundsService` | `stripe-compat` | `SecretKey` |
//! | 17 | **M8 (`v0.8.0`) Stripe Customers** | `client.customers(vault)` | `CustomersService` | `stripe-compat` | `SecretKey` + `CustomerVault` |
//! | 18 | **M8 (`v0.8.0`) Subscriptions & Unified** | `client.subscriptions(vault)` / `UnifiedPaymentGateway` | `SubscriptionsService` / `UnifiedPaymentGateway` | `stripe-compat` / `unified-gateway` | `SecretKey` |
//!
//! ## Feature-Flag Quick Reference
//!
//! | Feature Flag | Default | Description |
//! | :--- | :---: | :--- |
//! | `default` | **Yes** | Enables `core`, `env-keys`, `payments`, `payment-intents`, `tokens-invoices-3ds`, `settlements-ledger`, `payouts`, and `sources-webhooks` |
//! | `core` | **Yes** | Microkernel client, Basic Auth typestate keys, `given_id` idempotency, `Metadata`, `Amount`, `Currency`, `MoyasarError`, `Paginator` |
//! | `env-keys` | **Yes** | Resolves `$ENV_VAR` / `${ENV_VAR}` and `.env*` secret and publishable keys with zeroization |
//! | `payments` | **Yes** | `PaymentsService` (`create`, `retrieve`, `list`, `update`, `refund`, `capture`, `void`, `reverse`, splits, AFT, STC Pay OTP, SADAD) |
//! | `payment-intents` | **Yes** | `PaymentIntentsService` (`create`, `fulfill`, `retrieve`, `cancel`, 1-hour `is_usable()` expiry guard) |
//! | `tokens` | **Yes** | `TokensService` (`create`, `retrieve`, `delete` saved card tokens) |
//! | `invoices` | **Yes** | `InvoicesService` (`create`, `create_bulk`, `list`, `retrieve`, `update`, `cancel` hosted e-invoices) |
//! | `card-auths` (alias `card-auth`) | **Yes** | `CardAuthsService` (standalone 3D Secure authentication and MPI parameter extraction) |
//! | `tokens-invoices-3ds` | **Yes** | Umbrella feature enabling `tokens`, `invoices`, and `card-auths` |
//! | `payouts` | **Yes** | `PayoutAccountsService` and `PayoutsService` (single & bulk payouts across `internal`, `ips`, and `sarie`) |
//! | `settlements` / `settlements-ledger` | **Yes** | `SettlementsService`, `TransfersService`, and `InternalTransactionsService` |
//! | `sources` | **Yes** | `SourcesService` (BIN/IIN issuer lookup) and `ApplePayService` (Web merchant session initiation) |
//! | `webhooks-api` | **Yes** | `WebhooksService` (CRUD for `/v1/webhooks`, `available_events`, and delivery attempts) |
//! | `webhook-engine` | **Yes** | `ConstantTimeSecretVerifier`, replay-window validation, `InMemoryDeduplicator`, and `payment_faild` typo resilience |
//! | `sources-webhooks` | **Yes** | Umbrella feature enabling `sources`, `webhooks-api`, and `webhook-engine` |
//! | `stripe-compat` | No | `RefundsService`, `CustomersService`, `SubscriptionsService`, `InMemoryCustomerVault`, and pro-rata refund helpers |
//! | `unified` / `unified-gateway` (aliases `some-payments-trait`, `some-payment-trait`) | No | Cross-gateway `UnifiedPaymentGateway`, `PaymentCapability`, `PayoutCapability`, and `WebhookCapability` traits |
//! | `async-stream` | No | Asynchronous `futures::Stream` auto-pagination adapter over `Paginator` |
//! | `axum` / `actix` / `rocket` / `tonic-support` | Example / Dev | Reference server and E2E integration suites (`axum_server`, `actix_server`, `rocket_server`, `tonic_grpc_server`) enabled via `--features full` |
//! | `test-utils` | No | `MoyasarMockServer` (`wiremock`) and deterministic JSON fixtures for all 11+1 webhook events, STC Pay, SADAD, and ISO 8583 |
//! | `full` | No | Enables all production modules, `stripe-compat`, `unified-gateway`, and `async-stream` |
//!
//! ## Quick Start Examples
//!
//! ### Initializing a `MoyasarClient` with `SecretKey`
//!
//! ```no_run
//! use some_moyasar::prelude::{MoyasarClient, MoyasarError, SecretKey};
//!
//! fn main() -> Result<(), MoyasarError> {
//!     let key = SecretKey::new("sk_test_example_secret_key")?;
//!     let client = MoyasarClient::new(key)?;
//!     assert_eq!(client.base_url(), "https://api.moyasar.com");
//!     Ok(())
//! }
//! ```
//!
//! ### Converting Major Currency Units to Minor Units (`Halalas`)
//!
//! ```
//! use some_moyasar::prelude::{Amount, Currency};
//! # #[allow(dead_code)]
//! # trait AmountMinorExt {
//! #     fn as_minor(self) -> i64;
//! # }
//! # impl AmountMinorExt for Amount {
//! #     fn as_minor(self) -> i64 {
//! #         self.minor_units()
//! #     }
//! # }
//!
//! assert!(Amount::from_major(150, Currency::SAR).as_minor() == 15_000);
//! assert_eq!(Amount::from_major(150, Currency::SAR).minor_units(), 15_000);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![allow(clippy::incompatible_msrv)]

use chrono as _;

#[cfg(feature = "core")]
pub mod kernel;

#[cfg(feature = "core")]
pub mod prelude;

#[cfg(any(
    feature = "payments",
    feature = "payment-intents",
    feature = "tokens",
    feature = "invoices",
    feature = "card-auths",
    feature = "settlements-ledger",
    feature = "payouts",
    feature = "sources",
    feature = "webhooks-api"
))]
pub mod resources;

#[cfg(feature = "webhook-engine")]
pub mod webhook_engine;

#[cfg(feature = "stripe-compat")]
pub mod stripe_compat;

#[cfg(feature = "unified-gateway")]
pub mod unified;
#[cfg(feature = "unified-gateway")]
pub use crate::unified::*;

#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;
#[cfg(any(test, feature = "test-utils"))]
pub use crate::test_utils::mock::MoyasarMockServer;
