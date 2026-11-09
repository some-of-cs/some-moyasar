# User Story Specifications Index

Generated from [`docs/Moyasar Rust SDK Research.md`](../Moyasar%20Rust%20SDK%20Research.md) via the
`spec-decomposition` skill. Module, story and unit numbering is authoritative in
[`DECOMPOSITION_ROADMAP.md`](../architecture/DECOMPOSITION_ROADMAP.md).

- **8 modules · 23 stories · 99 implementation units**
- Every story file follows the same six-section template:
  1. Spec Kit Specification (narrative, pre/postconditions, Gherkin ACs, traceability matrix)
  2. Behavior-Centric Architecture & Folder Blueprint
  3. Technical Implementation Plan (invariants, schema, sequence diagram)
  4. Granular Implementation Tasks (10-PR vertical slicing table + phase breakdown)
  5. Security Gateway (trust boundaries, STRIDE/ASVS matrix, crypto controls, G2 checklist)
  6. Validation Gates (G0–G4 + G-Review + G-Borrow + G-Complexity + G-Docs + G-Law)

## Module 1 — Core Microkernel & Transport Layer (11 units)

| Story | Title | Units | File |
| :--- | :--- | :-- | :--- |
| US-1.1.0 | HTTP Client, Basic Auth & TLS | 3 | [link](module-1-core-microkernel/US-1.1.0-http-client-auth.md) |
| US-1.2.0 | Idempotency, Metadata & Pagination | 5 | [link](module-1-core-microkernel/US-1.2.0-idempotency-metadata-pagination.md) |
| US-1.3.0 | Error Types & Prelude | 3 | [link](module-1-core-microkernel/US-1.3.0-error-types-prelude.md) |

## Module 2 — Payments Service (24 units)

| Story | Title | Units | File |
| :--- | :--- | :-- | :--- |
| US-2.1.0 | Payment Create, Retrieve & List | 5 | [link](module-2-payments/US-2.1.0-payment-create-retrieve-list.md) |
| US-2.2.0 | Payment Lifecycle Operations | 5 | [link](module-2-payments/US-2.2.0-payment-lifecycle-operations.md) |
| US-2.3.0 | Source Request Types | 6 | [link](module-2-payments/US-2.3.0-source-request-types.md) |
| US-2.4.0 | STC Pay & SADAD Bill Sources | 4 | [link](module-2-payments/US-2.4.0-source-stcpay-sadad.md) |
| US-2.5.0 | Marketplace Splits & AFT | 4 | [link](module-2-payments/US-2.5.0-marketplace-splits-aft.md) |

## Module 3 — Payment Intents (8 units)

| Story | Title | Units | File |
| :--- | :--- | :-- | :--- |
| US-3.1.0 | Intent Create, Retrieve & Cancel | 4 | [link](module-3-payment-intents/US-3.1.0-intent-create-retrieve-cancel.md) |
| US-3.2.0 | Intent Fulfillment | 4 | [link](module-3-payment-intents/US-3.2.0-intent-fulfill.md) |

## Module 4 — Tokenization, Invoices & 3D Secure (12 units)

| Story | Title | Units | File |
| :--- | :--- | :-- | :--- |
| US-4.1.0 | Card Tokenization | 4 | [link](module-4-tokens-invoices-3ds/US-4.1.0-card-tokens.md) |
| US-4.2.0 | Hosted E-Invoices | 5 | [link](module-4-tokens-invoices-3ds/US-4.2.0-invoices.md) |
| US-4.3.0 | Standalone 3DS Card Authentication | 3 | [link](module-4-tokens-invoices-3ds/US-4.3.0-card-auth-3ds.md) |

## Module 5 — Payouts & Payout Accounts (10 units)

| Story | Title | Units | File |
| :--- | :--- | :-- | :--- |
| US-5.1.0 | Payout Accounts | 4 | [link](module-5-payouts/US-5.1.0-payout-accounts.md) |
| US-5.2.0 | Payout Disbursements & Purpose Codes | 6 | [link](module-5-payouts/US-5.2.0-payout-disbursements.md) |

## Module 6 — Settlements, Transfers & Ledger (9 units)

| Story | Title | Units | File |
| :--- | :--- | :-- | :--- |
| US-6.1.0 | Settlements, Lines & Balance | 5 | [link](module-6-settlements-ledger/US-6.1.0-settlements.md) |
| US-6.2.0 | Transfers & Internal Transactions | 4 | [link](module-6-settlements-ledger/US-6.2.0-transfers-internal-txns.md) |

## Module 7 — Sources, Apple Pay & Webhooks (13 units)

| Story | Title | Units | File |
| :--- | :--- | :-- | :--- |
| US-7.1.0 | Generic Sources & Apple Pay | 3 | [link](module-7-sources-webhooks/US-7.1.0-sources-applepay.md) |
| US-7.2.0 | Webhooks Management API | 5 | [link](module-7-sources-webhooks/US-7.2.0-webhooks-api.md) |
| US-7.3.0 | Webhook Verification Engine | 5 | [link](module-7-sources-webhooks/US-7.3.0-webhook-engine.md) |

## Module 8 — Cross-Cutting Compatibility (12 units)

| Story | Title | Units | File |
| :--- | :--- | :-- | :--- |
| US-8.1.0 | Stripe-Compat Refunds & Customers | 4 | [link](module-8-cross-cutting/US-8.1.0-stripe-compat-refunds-customers.md) |
| US-8.2.0 | Stripe-Compat Subscriptions | 5 | [link](module-8-cross-cutting/US-8.2.0-stripe-compat-subscriptions.md) |
| US-8.3.0 | `some-payment` Trait & Conformance Suite | 3 | [link](module-8-cross-cutting/US-8.3.0-some-payment-trait-tests.md) |

## Totals

| Module | Stories | Units |
| :--- | :-- | :-- |
| 1 — Core Microkernel | 3 | 11 |
| 2 — Payments | 5 | 24 |
| 3 — Payment Intents | 2 | 8 |
| 4 — Tokens / Invoices / 3DS | 3 | 12 |
| 5 — Payouts | 2 | 10 |
| 6 — Settlements & Ledger | 2 | 9 |
| 7 — Sources & Webhooks | 3 | 13 |
| 8 — Cross-Cutting | 3 | 12 |
| **Total** | **23** | **99** |

## Branch Naming

Each story maps to one branch following the roadmap convention:

```
feat/us-<M>.<S>.0-<slug>
```

e.g. `feat/us-7.3.0-webhook-verification-engine`.

## 2. The 10-PR Vertical Slicing Framework (Rust SDK Adaptation)

Under the `some-divide-and-conquer` dynamic decomposition engine ([`DECOMPOSITION_ROADMAP.md`](../architecture/DECOMPOSITION_ROADMAP.md) and [`.agents/rules/02-monolith-to-units-decomposition.md`](../../.agents/rules/02-monolith-to-units-decomposition.md)), each User Story (`feat/us-M.S.0-<slug>`) is dynamically sized via $S_{\text{story}}$ into $K \in [3, 6]$ parallel, non-blocking Implementation Units (`feat/us-M.S.0/unit-1-<scope>` through `unit-K-<scope>`):

1. **Base Branch Scaffolding**: Before spawning unit worktrees, the Module/Story Leader provisions `Cargo.toml`, `src/lib.rs`, `src/kernel/mod.rs`, `src/resources/mod.rs`, and the domain's `mod.rs` + `service.rs` facade on the base branch.
2. **Disjoint File Ownership (Invariant 2)**: Every unit $u_i \in \{1..K\}$ owns an exclusive set of production `.rs` files and never appends to a sibling unit's file in the same wave.
3. **Pure Deliverable Production Only (Invariant 1)**: Every unit ships real production Rust code plus colocated `#[cfg(test)]` unit tests; story-wide integration wire tests execute after all $K$ units merge into the story branch.
