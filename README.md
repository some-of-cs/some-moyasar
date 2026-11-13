# some-moyasar

<div align="center">

> **Idiomatic, microkernel-architected Rust SDK for the currently implemented Moyasar API resources.**
> Modeled after [`stripe-node`](https://github.com/stripe/stripe-node) ergonomics • Built for the Rust 2024 edition • Core GCC & Saudi Arabia provider for [`some-payment`](https://github.com/some-of-cs/some-payment)

[![Crates.io](https://img.shields.io/crates/v/some-moyasar.svg)](https://crates.io/crates/some-moyasar)
[![Docs.rs](https://docs.rs/some-moyasar/badge.svg)](https://docs.rs/some-moyasar)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust: Edition 2024](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://github.com/some-of-cs/some-moyasar)
[![Ecosystem: some-payment](https://img.shields.io/badge/ecosystem-some--payment-8a2be2.svg)](https://github.com/some-of-cs/some-payment)
[![Sponsor](https://img.shields.io/badge/Sponsor-%E2%99%A5-ff69b4.svg)](https://github.com/sponsors/some-of-cs)

[Why some-moyasar?](#why-some-moyasar) •
[Key Features](#key-features) •
[Feature Flags](#comprehensive-feature-flag-matrix) •
[Quick Start](#quick-start) •
[Webhook Security](#constant-time-webhook-verification-engine) •
[API Coverage](#complete-moyasar-services--api-coverage) •
[some-payment Ecosystem](#cross-gateway-interoperability-with-some-payment) •
[Sponsors](#backers--corporate-sponsors)

</div>

---

> **Implementation status:** This crate currently ships the `core`, `payments`,
> `payment-intents`, `tokens`, `invoices`, and `card-auths` feature modules.
> Webhook verification, payouts, settlements, Apple Pay, Stripe compatibility,
> and `some-payment` integration described elsewhere in this document are
> roadmap/research material and are not part of the current published API.
> Do not treat this repository as PCI DSS, payment-services, AML/KYC, sanctions,
> privacy, or regulatory compliance certification.

## Why some-moyasar?

Building high-concurrency Rust backend systems for Saudi Arabia and GCC commerce with the [Moyasar API](https://docs.moyasar.com/api/api-introduction) presents critical engineering challenges:

1. **Key-Type Isolation (`sk_` vs `pk_`)**: [Moyasar Authentication](https://docs.moyasar.com/api/authentication) uses HTTP Basic Auth (`Authorization: Basic base64("{api_key}:")`) with strict permission boundaries—restricting publishable keys (`pk_...`) to 4 client-originated routes while returning `403 Forbidden` if a publishable key attempts to read a [PaymentIntent](https://docs.moyasar.com/guides/payment-intents/overview) containing a `client_secret`. `some-moyasar` enforces `SecretKey` and `PublishableKey` separation at compile time.
2. **Body-Injected UUID Idempotency**: Unlike Stripe's header-driven model, [Moyasar Idempotency](https://docs.moyasar.com/api/idempotency) requires a client-generated UUID (`given_id`) inside the JSON payload and assigns that UUID as the resulting `Payment.id`. `some-moyasar` automatically injects deterministic UUIDv4/v5 `given_id` tokens so network timeouts and 5xx errors can be retried safely without double-charging customers.
3. **Zero-Leak Webhook Verification & Typo Resilience**: [Moyasar Webhooks](https://docs.moyasar.com/api/other/webhooks/webhook-reference) embed `secret_token` inside the JSON payload and exhibit a documented spelling variance between `payment_failed` and `payment_faild` across their [Webhook Reference](https://docs.moyasar.com/api/other/webhooks/webhook-reference). `some-moyasar` verifies `secret_token` in strict constant time via double-HMAC-SHA256 (`subtle::ConstantTimeEq`) to eliminate both timing side-channels and secret-length leakage while transparently deserializing both `payment_failed` and `payment_faild`.
4. **Drop-In Multi-Gateway Orchestration**: Engineered as a first-class provider crate within [`some-payment`](https://github.com/some-of-cs/some-payment) alongside [`some-stripe`](https://github.com/some-of-cs/some-stripe) and [`some-paymob`](https://github.com/some-of-cs/some-paymob), enabling dynamic country-based gateway switching (e.g., routing `SA` checkouts to Mada/STC Pay/SADAD via Moyasar while sharing Stripe Identity KYC).

---

## Key Features

- **Microkernel Namespace Ergonomics**: Modeled after [`stripe-node`](https://github.com/stripe/stripe-node) (`client.payments()`, `client.payment_intents()`, `client.payouts()`, `client.invoices()`, `client.tokens()`, `client.settlements()`) over a cheap-to-clone `MoyasarClient`.
- **Full Saudi & GCC Payment Instruments**: Strongly typed polymorphic sources for Credit/Debit Cards (`mada`, `visa`, `master`, `amex`), [Apple Pay Web Merchant Sessions](https://docs.moyasar.com/api/other/apple-pay/request-apple-pay-session), Samsung Pay, [STC Pay OTP flows](https://docs.moyasar.com/guides/stc-pay/custom-ui), [SADAD Bills](https://docs.moyasar.com/guides/sadad-bill/basic-integration), and [Saved Card Tokens](https://docs.moyasar.com/guides/tokenization/tokenized-cards).
- **Two-Step Payment Intents & Expiry Guard**: Full support for [Moyasar Payment Intents](https://docs.moyasar.com/guides/payment-intents/intent-lifecycle) with `.is_usable()` helpers that guard against Moyasar's silent 1-hour `expires_at` window where expired intents remain in `"status": "created"`.
- **Regulated SARIE & IPS Bulk Payouts**: Built-in support for [Moyasar Payouts](https://docs.moyasar.com/api/payouts/05-list-payouts) across `internal`, `ips`, and `sarie` channels with compile-time enforcement of all 20 Saudi-regulated payout purpose classifications.
- **Marketplace Splits & AFT**: Native support for marketplace split settlements (`Entity`, `Platform`, `Beneficiary`) on [Create Payment](https://docs.moyasar.com/api/payments/01-create-payment) and [Account Funding Transactions (AFT)](https://docs.moyasar.com/guides/account-funding-transactions/introduction).
- **Stripe-Parity Compatibility Layer (`stripe-compat`)**: Virtual top-level `client.refunds()`, token-backed `client.customers()`, and `client.subscriptions()` orchestrators with pro-rata refund calculation and automatic 2-hour unsettled void fallback per [Moyasar Payment Operations](https://docs.moyasar.com/guides/payment-operations).

---

## Comprehensive Feature Flag Matrix

| Feature Flag | Default | Dependencies Added | Included Modules, Services & Capabilities |
| :--- | :---: | :--- | :--- |
| `default` | **Yes** | `reqwest`, `serde`, `uuid`, `secrecy`, `thiserror` | `core`, `payments`, `payment-intents`, `webhook-engine`, `rustls-tls` ([API Intro](https://docs.moyasar.com/api/api-introduction)) |
| `core` | **Yes** | `serde`, `serde_json`, `uuid`, `secrecy`, `chrono` | `MoyasarClient`, [Basic Auth](https://docs.moyasar.com/api/authentication) builder, [`given_id` idempotency](https://docs.moyasar.com/api/idempotency), [Metadata](https://docs.moyasar.com/api/metadata) validator, `MoyasarError` |
| `payments` | **Yes** | `core` | `client.payments()` — [Create](https://docs.moyasar.com/api/payments/01-create-payment), Fetch, [List](https://docs.moyasar.com/api/payments/03-list-payments), Update, [Refund](https://docs.moyasar.com/api/payments/05-refund-payment), Capture, Void; splits & [AFT](https://docs.moyasar.com/guides/account-funding-transactions/introduction) |
| `payment-intents` | **Yes** | `core`, `payments` | `client.payment_intents()` — Create, [Fulfill](https://docs.moyasar.com/api/payment_intents/02-fulfill-payment-intent), Fetch, [Cancel](https://docs.moyasar.com/api/payment_intents/04-cancel-payment-intent), and [1-hour expiry](https://docs.moyasar.com/guides/payment-intents/intent-lifecycle) helpers |
| `webhook-engine` | **Yes** | `hmac`, `sha2`, `subtle` | `ConstantTimeSecretVerifier` — Zero-leak constant-time `secret_token` verification & `payment_faild` alias ([Webhook Docs](https://docs.moyasar.com/api/other/webhooks/webhook-reference)) |
| `tokens` | No | `core` | `client.tokens()` — [Create](https://docs.moyasar.com/api/other/tokens/create-token), [Fetch](https://docs.moyasar.com/api/other/tokens/fetch-token), Delete (`204 No Content`) card tokens ([Token Status](https://docs.moyasar.com/api/other/tokens/token-status-reference)) |
| `invoices` | No | `core`, `payments` | `client.invoices()` — [Create](https://docs.moyasar.com/api/invoices/01-create-invoice), Bulk Create, List, [Fetch](https://docs.moyasar.com/api/invoices/04-show-invoice), [Update](https://docs.moyasar.com/api/invoices/05-update-invoice), [Cancel (`PUT`)](https://docs.moyasar.com/api/invoices/06-cancel-invoice) |
| `card-auth` | No | `core` | `client.card_auths()` — Standalone [3D Secure](https://docs.moyasar.com/category/3d-secure) (`POST /v1/card_auths`, [Fetch Card Auth](https://docs.moyasar.com/api/card_auths/02-fetch-card-auth)) |
| `payouts` | No | `core` | `client.payout_accounts()` & `client.payouts()` — Bank/Wallet accounts, single & bulk payouts across `internal`, `ips`, and `sarie` ([List Payouts](https://docs.moyasar.com/api/payouts/05-list-payouts)) |
| `settlements` | No | `core` | `client.settlements()`, `client.transfers()`, `client.internal_transactions()` — [Settlements](https://docs.moyasar.com/api/settlements/01-list-settlements), [Transfers](https://docs.moyasar.com/api/other/transfers/list-transfers), and [Internal Transactions](https://docs.moyasar.com/api/internal_transactions/02-list-internal-transactions) |
| `sources` | No | `core` | `client.sources()` & `client.apple_pay()` — Pre-charge [BIN/IIN Issuer Lookup](https://docs.moyasar.com/api/sources/01-get-issuer) and [Apple Pay Session](https://docs.moyasar.com/api/other/apple-pay/request-apple-pay-session) |
| `webhooks-api` | No | `core`, `webhook-engine` | `client.webhooks()` — CRUD management for `/v1/webhooks`, `available_events`, and delivery attempts ([Webhook Reference](https://docs.moyasar.com/api/other/webhooks/webhook-reference)) |
| `stripe-compat` | No | `payments`, `tokens`, `invoices` | `client.refunds()`, `client.customers()`, `client.subscriptions()` — Virtual refund service, token vault, and pro-rata subscription refunds ([Tokenized Cards](https://docs.moyasar.com/guides/tokenization/tokenized-cards)) |
| `some-payment-trait` | No | `some-payment-core` | Implements `PaymentCapability`, `PayoutCapability`, and `WebhookCapability` traits for [`some-payment`](https://github.com/some-of-cs/some-payment) |
| `full` | No | All of the above | Enables every Moyasar API resource, webhook engine, auto-pagination stream, and [`some-payment`](https://github.com/some-of-cs/some-payment) adapter |

---

## Installation

### Standard Configuration (Core Payments, Intents & Webhook Verification)

```toml
[dependencies]
some-moyasar = "0.1.0"
tokio = { version = "1", features = ["full"] }
```

### Enterprise Full-Suite Configuration (Marketplace Splits, SARIE Payouts & `some-payment` Adapter)

```toml
[dependencies]
some-moyasar = { version = "0.1.0", features = [
    "full",
    "stripe-compat",
    "some-payment-trait"
] }
```

---

## Quick Start

### 1. Creating a Payment with Marketplace Splits & Deterministic Idempotency

```rust,no_run
use some_moyasar::prelude::*;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), MoyasarError> {
    // Initialize the microkernel client with a SecretKey (Basic Auth with empty password)
    let client = MoyasarClient::builder(SecretKey::new("sk_test_YOUR_SECRET_KEY")?)
        .max_network_retries(2) // Safe retries enabled via body-injected `given_id`
        .build()?;

    // Construct validated Metadata (enforces <= 30 keys, key <= 40 chars, val <= 500 chars)
    let mut metadata = Metadata::new();
    metadata.insert("order_id", "ORD-2026-9941")?;
    metadata.insert("tenant_id", "vendor_alpha")?;

    // Create a payment with saved token and marketplace splits
    let create_params = CreatePaymentParams::builder()
        .given_id(Uuid::new_v4()) // Idempotency key & resulting Payment ID
        .amount(15000)            // 150.00 SAR in halalas (smallest currency unit)
        .currency(Currency::SAR)
        .description("Annual Pro Plan + Vendor Split")
        .callback_url("https://merchant.example/payments/callback")
        .source(PaymentSourceRequest::Token(TokenSource {
            token: "token_qbmmXzo97AESrZLS6KpWvof6uK2hAKcQGfEcKg".into(),
            three_ds: Some(false),
            manual: Some(false),
            cvc: None,
        }))
        .splits(vec![
            PaymentSplitRequest {
                amount: 13500,
                recipient_type: SplitRecipientType::Beneficiary,
                recipient_id: Uuid::parse_str("3fa85f64-5717-4562-b3fc-2c963f66afa6").unwrap(),
                reference: Some("SPLIT-VENDOR-9941".into()),
                description: Some("85% Vendor Revenue Share".into()),
                fee_source: Some(false),
                refundable: Some(true),
            },
            PaymentSplitRequest {
                amount: 1500,
                recipient_type: SplitRecipientType::Platform,
                recipient_id: Uuid::parse_str("ae50a35c-df42-4eff-ba26-f8bc28d2af81").unwrap(),
                reference: Some("SPLIT-PLATFORM-FEE".into()),
                description: Some("15% Platform Commission".into()),
                fee_source: Some(true), // Deduct Moyasar processing fees from platform share
                refundable: Some(true),
            },
        ])
        .metadata(metadata)
        .build();

    let payment = client.payments().create(create_params).await?;
    println!("Payment {} status: {:?} ({})", payment.id, payment.status, payment.amount_format);
    Ok(())
}
```

### 2. Two-Step Payment Intents Lifecycle (Backend Creation & Expiry-Safe Inspection)

Per [Moyasar's Payment Intents Guide](https://docs.moyasar.com/guides/payment-intents/intent-lifecycle), intents lock `amount`, `currency`, `description`, and `callback_url` on the backend and expire 1 hour after creation (`expires_at`), while keeping `"status": "created"`:

```rust,no_run
use some_moyasar::prelude::*;

async fn manage_payment_intent(client: &MoyasarClient) -> Result<(), MoyasarError> {
    let intent = client.payment_intents().create(CreatePaymentIntentParams {
        amount: 25000, // 250.00 SAR
        currency: Currency::SAR,
        description: Some("Checkout Session #882".into()),
        callback_url: "https://merchant.example/return".into(),
    }).await?;

    let fetched = client.payment_intents().retrieve(intent.id).await?;

    if fetched.is_usable() {
        // Safe to cancel because status == Created AND Utc::now() < fetched.expires_at
        let canceled = client.payment_intents().cancel(fetched.id).await?;
        assert_eq!(canceled.status, PaymentIntentStatus::Canceled);
    }
    Ok(())
}
```

### 3. Regulated SARIE & IPS Bulk Payouts

Disburse funds to Saudi bank IBANs or mobile wallets via [`/v1/payouts/bulk`](https://docs.moyasar.com/api/payouts/05-list-payouts) using compile-time validated `PayoutPurpose` and `PayoutDestination` types:

```rust,no_run
use some_moyasar::prelude::*;
use uuid::Uuid;

async fn disburse_vendor_payouts(client: &MoyasarClient, source_account_id: Uuid) -> Result<(), MoyasarError> {
    let bulk_response = client.payouts().create_bulk(CreateBulkPayoutParams {
        source_id: source_account_id,
        payouts: vec![
            BulkPayoutEntry {
                sequence_number: Some("2026110500000001".into()),
                amount: 125000, // 1,250.00 SAR
                purpose: PayoutPurpose::PaymentToMerchant,
                destination: PayoutDestination::Bank {
                    iban: "SA8430400108057386290038".into(),
                    name: "Al-Noor Trading Est".into(),
                    mobile: "+966501234567".into(),
                    country: "SA".into(),
                    city: "Riyadh".into(),
                },
                comment: Some("Weekly Marketplace Settlement".into()),
                metadata: None,
            },
        ],
    }).await?;

    for payout in bulk_response.payouts {
        println!("Payout {} via {:?}: {:?}", payout.id, payout.channel, payout.status);
    }
    Ok(())
}
```

---

## Constant-Time Webhook Verification Engine

Unlike header-signed webhooks, [Moyasar Webhooks](https://docs.moyasar.com/api/other/webhooks/webhook-reference) embed `secret_token` inside the JSON body and retry failed deliveries across 6 attempts spanning 3 hours and 41 minutes (`1m`, `10m`, `30m`, `1h`, `2h`). `some-moyasar` hashes both the incoming `secret_token` and your configured `shared_secret` with a fixed domain-separation key via HMAC-SHA256 before comparing fixed 32-byte digests with `subtle::ConstantTimeEq`, preventing both timing attacks and secret-length leakage:

```rust
use some_moyasar::webhook_engine::{ConstantTimeSecretVerifier, WebhookEventType};
use std::time::Duration;

fn verify_moyasar_webhook(raw_body: &[u8], shared_secret: &str) {
    // 4-hour tolerance accommodates Moyasar's 3h41m 6-attempt retry schedule
    let verifier = ConstantTimeSecretVerifier::new(shared_secret, Duration::from_secs(4 * 3600));

    match verifier.verify_and_construct(raw_body) {
        Ok(event) => {
            match event.event_type {
                WebhookEventType::PaymentPaid => println!("Payment succeeded: {}", event.id),
                // Automatically matches both `payment_failed` and `payment_faild`
                WebhookEventType::PaymentFailed => eprintln!("Payment failed: {}", event.id),
                _ => println!("Received event {:?}", event.event_type),
            }
        }
        Err(err) => eprintln!("Rejected webhook: {err}"),
    }
}
```

---

## Complete Moyasar Services & API Coverage

| Service Domain | Accessor | Endpoints Covered | Spec Reference |
| :--- | :--- | :--- | :--- |
| **Payments** | `client.payments()` | `POST /v1/payments`, `GET /v1/payments/:id`, `GET /v1/payments`, `PUT /v1/payments/:id`, `POST /:id/refund`, `POST /:id/capture`, `POST /:id/void`, STC Pay OTP | [Payments API](https://docs.moyasar.com/category/payments-api) |
| **Payment Intents** | `client.payment_intents()` | `POST /v1/payment_intents`, `POST /v1/payment_intents/:id/fulfill`, `GET /v1/payment_intents/:id`, `POST /v1/payment_intents/:id/cancel` | [Payment Intents](https://docs.moyasar.com/guides/payment-intents/overview) |
| **Tokens** | `client.tokens()` | `POST /v1/tokens`, `GET /v1/tokens/:id`, `DELETE /v1/tokens/:id` (`204 No Content`) | [Tokenization](https://docs.moyasar.com/category/tokenization) |
| **Invoices** | `client.invoices()` | `POST /v1/invoices`, `POST /v1/invoices/bulk`, `GET /v1/invoices`, `GET /v1/invoices/:id`, `PUT /v1/invoices/:id`, `PUT /v1/invoices/:id/cancel` | [Invoices API](https://docs.moyasar.com/category/invoices) |
| **Standalone 3DS** | `client.card_auths()` | `POST /v1/card_auths`, `GET /v1/card_auths/:id` (ECI, CAVV, DS Transaction ID) | [3D Secure](https://docs.moyasar.com/category/3d-secure) |
| **Payouts & Accounts** | `client.payouts()`, `client.payout_accounts()` | `POST /v1/payout_accounts`, `GET /v1/payout_accounts`, `POST /v1/payouts`, `POST /v1/payouts/bulk`, `GET /v1/payouts` | [List Payouts](https://docs.moyasar.com/api/payouts/05-list-payouts) |
| **Settlements & Ledger** | `client.settlements()`, `client.transfers()`, `client.internal_transactions()` | `GET /v1/settlements`, `GET /v1/settlements/:id/lines`, `GET /v1/settlements/balance`, `GET /v1/transfers`, `GET /v1/internal_transactions` | [List Settlements](https://docs.moyasar.com/api/settlements/01-list-settlements) |
| **Sources & Apple Pay** | `client.sources()`, `client.apple_pay()` | `POST /v1/source/issuer` (BIN/IIN lookup), `GET/POST /v1/applepay/initiate` | [Retrieve Issuer](https://docs.moyasar.com/api/sources/01-get-issuer) |
| **Webhooks API** | `client.webhooks()` | `POST /v1/webhooks`, `GET /v1/webhooks`, `GET /v1/webhooks/available_events`, `GET /v1/webhooks/attempts` | [Webhook Reference](https://docs.moyasar.com/api/other/webhooks/webhook-reference) |

---

## Cross-Gateway Interoperability with `some-payment`

`some-moyasar` is part of the unified **[`some-payment`](https://github.com/some-of-cs/some-payment)** ecosystem alongside **[`some-stripe`](https://github.com/some-of-cs/some-stripe)** and **[`some-paymob`](https://github.com/some-of-cs/some-paymob)**. Enable the `some-payment-trait` feature to plug `MoyasarClient` directly into `some-payment`'s country-aware router and cross-gateway capability composer—for example:

- Routing Saudi Arabia (`SA`) checkouts and SARIE/IPS disbursements to **`some-moyasar`**, Egypt (`EG`) checkouts and mobile wallet cash-ins to **[`some-paymob`](https://github.com/some-of-cs/some-paymob)**, and global (`US`/`EU`/`GB`) checkouts to **[`some-stripe`](https://github.com/some-of-cs/some-stripe)**.
- Composing **[`some-stripe`](https://github.com/some-of-cs/some-stripe)** Identity KYC (`VerificationSession`) with **`some-moyasar`** Mada/STC Pay payment collection and SARIE vendor payouts in a single unified workflow.

---

## Backers & Corporate Sponsors

`some-moyasar` (and the [`some-payment`](https://github.com/some-of-cs/some-payment) / [`some-stripe`](https://github.com/some-of-cs/some-stripe) ecosystem) is an independent, community-driven open-source project. If your business relies on this crate to process transactions or verify webhooks, corporate sponsorship directly finances continuous maintenance, zero-day security updates, and upcoming module expansion.

<div align="center">

[![Become a Sponsor](https://img.shields.io/badge/Become%20a%20Sponsor-%E2%99%A5-ea4aaa?style=for-the-badge)](https://github.com/sponsors/some-of-cs)

</div>

### Corporate Sponsorship Tiers & Benefits

| Tier | Monthly | Placement & Benefits |
| :--- | :--- | :--- |
| **🏆 Visionary** | $1,000 / mo | Large logo on `README.md`, website, and Docs.rs header • Prioritized GitHub issues • Dedicated quarterly roadmap review |
| **🥇 Gold Partner** | $500 / mo | Prominent logo on `README.md` • Prioritized PR reviews • Direct issue escalation |
| **🥈 Silver Backer** | $150 / mo | Company logo, link, and mention in `README.md` and release changelogs |
| **☕ Individual** | $10 – $50 / mo | Listed as a contributor and backer on GitHub |

### Sponsors & Backers Wall

<div align="center">

| 🏆 Visionary Sponsors | 🥇 Gold Partners | 🥈 Silver Backers | ☕ Individual Backers |
| :---: | :---: | :---: | :---: |
| *Your company logo here*<br>[Become a Visionary Sponsor](https://github.com/sponsors/some-of-cs) | *Your company logo here*<br>[Become a Gold Partner](https://github.com/sponsors/some-of-cs) | *Your company logo here*<br>[Become a Silver Backer](https://github.com/sponsors/some-of-cs) | [Join our Individual Backers](https://github.com/sponsors/some-of-cs) |

</div>

> [!TIP]
> **Need an Invoice or Procurement Documentation?**  
> We can issue formal VAT/sales-tax compliant invoices via Stripe Invoicing for corporate sponsors. Please contact [sponsors@someofcs.tech](mailto:sponsors@someofcs.tech).

---

## Contributing

We welcome community contributions! To ensure code quality and consistency:

1. Ensure all tests and doc-tests pass:
   ```bash
   cargo test
   ```
2. Ensure code compiles without warnings on the Rust 2024 edition:
   ```bash
   cargo clippy --all-targets -- -D warnings
   ```
3. Format all code according to standard `rustfmt`:
   ```bash
   cargo fmt --check
   ```

---

## License

Licensed under the Apache License, Version 2.0 ([LICENSE](LICENSE)).
