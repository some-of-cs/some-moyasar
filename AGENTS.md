# some-moyasar (v0.10.0) — Agent Guide

Zero-`unsafe` (`#![forbid(unsafe_code)]`) microkernel Rust SDK (MSRV `1.85`, edition `2024`).

## Quick Setup

```toml
# Default
some-moyasar = "0.10.0"
# Minimal payments-only
some-moyasar = { version = "0.10.0", default-features = false, features = ["payments"] }
# some-payment-core adapter
some-moyasar = { version = "0.10.0", features = ["some-payment-adapter"] }
# Full SDK
some-moyasar = { version = "0.10.0", features = ["full"] }
```

|Feature|Def|Scope|
|:--|:--:|:--|
|core|Y|Client, keys, Amount, Currency, errors|
|env-keys|Y|`$ENV`/`${ENV}` & `.env*` keys|
|payments|Y|Payments, splits, AFT, STC, SADAD|
|payment-intents|Y|Intents (1h expiry)|
|tokens-invoices-3ds|Y|tokens, invoices, card-auths (card-auth)|
|payouts|Y|Accounts & payouts (internal/ips/sarie)|
|settlements-ledger|Y|Alias settlements: ledger & transfers|
|sources-webhooks|Y|sources, webhooks-api, webhook-engine|
|stripe-compat|N|Refunds, customers, subscriptions|
|unified-gateway|N|Aliases some-payment(s)-trait|
|some-payment-adapter|N|MoyasarProvider (PaymentProvider)|
|async-stream|N|futures::Stream pagination|
|test-utils|N|MoyasarMockServer (wiremock)|
|full|N|Default + compat, unified, stream|

## Core Invariants

- Minor units: `Amount(i64)` / `u64` (100 halalas = 1 SAR; zero floats, zero `unsafe`).
- Keys: `SecretKey` (`sk_test_`/`sk_live_`) for server mutations; `PublishableKey` (`pk_test_`/`pk_live_`) for client tokenization, Apple Pay, payment/intent creation. `AuthMode::Live` enforces HTTPS.
- Idempotency: `given_id` UUID (v4 auto or v5 via `idempotency_v5::derive_subscription_given_id`).
- Webhooks: Constant-time (`ConstantTimeSecretVerifier` / `verify_and_scrub_moyasar_webhook`), recursive `secret_token` scrubbing, `payment_faild` typo tolerance, `InMemoryDeduplicator`.
- Redaction: Zero PAN/CVC logs; `Debug` prints `[REDACTED]`, secrets zeroize on drop.

## Minimal Patterns

1. Client & payments:

```rust
use some_moyasar::prelude::*;

async fn pay() -> Result<(), MoyasarError> {
    let c = MoyasarClient::new("$MOYASAR_SECRET_KEY")?; // or SecretKey::new("sk_test_...")?
    let p = CreatePaymentParams::builder()
        .amount(15_000)
        .currency(Currency::SAR)
        .payment_source(TokenSource::new("token_x")?)
        .build()?;
    let p = c.payments().create(&p).await?;
    let p = c.payments().retrieve(&PaymentId::new(&p.id)?).await?;
    let _ = c.payments().capture(&p.id, Some(15_000)).await?;
    let _ = c.payments().refund(&p.id, Some(5_000)).await?;
    let _ = c.payments().void(&p.id).await?;
    Ok(())
}
```

2. Webhook verification & dedup:

```rust
use some_moyasar::prelude::*;

async fn on_hook(body: &[u8], secret: &str) -> Result<bool, WebhookError> {
    let v = ConstantTimeSecretVerifier::with_default_tolerance(secret);
    let d = InMemoryDeduplicator::new();
    let ev = v.verify_and_construct(body)?;
    d.check_and_record(ev.id).await
}
```

3. `some-payment-adapter`:

```rust
use some_moyasar::adapter::moyasar_provider::MoyasarProvider;
use some_payment_core::{PaymentError, PaymentProvider};

fn provider(key: &str) -> Result<impl PaymentProvider, PaymentError> {
    MoyasarProvider::from_api_key(key)
}
```

## Module Map

|Module|Role|
|:--|:--|
|kernel|Client, auth, Amount, idempotency_v5|
|prelude|Re-exports for enabled features|
|resources::payments|CRUD, capture, void, refund|
|resources::payment_intents|Intents (create/fulfill/retrieve/cancel)|
|resources::tokens|Saved card tokens|
|resources::invoices|Hosted e-invoices|
|resources::card_auths|Standalone 3DS & MPI|
|resources::payouts|Payout accounts & payouts|
|resources::settlements|Settlements, transfers, ledger|
|resources::sources|BIN/IIN & Apple Pay|
|resources::webhooks_api|/v1/webhooks CRUD & attempts|
|webhook_engine|Verifier, replay, dedup|
|stripe_compat|Refunds, customers, subscriptions|
|unified|UnifiedPaymentGateway|
|alignment|Guards & verify_and_scrub_moyasar_webhook|
|adapter|moyasar_provider::MoyasarProvider|
|test_utils|MoyasarMockServer & fixtures|

Examples (`--features full`): `examples/{actix,axum,rocket,tonic}`.

## Offline Testing

```bash
cargo test --features test-utils
cargo test --all-features
cargo test --doc --all-features
```
