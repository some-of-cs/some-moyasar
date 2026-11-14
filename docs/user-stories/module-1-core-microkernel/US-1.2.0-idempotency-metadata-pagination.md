# US-1.2.0: Body-Injected `given_id` Idempotency, Validated Metadata & Pagination

> **Spec Kit Specification**: [Specification Lifecycle Active](../../../docs/architecture/DECOMPOSITION_ROADMAP.md)  
> **Behavior Blueprint**: `src/kernel/`  
> **Slicing Standard**: [10-PR Vertical Slicing Framework](../README.md#2-the-10-pr-vertical-slicing-framework-rust-sdk-adaptation)  
> **Security Gateway**: `security-audit` Level 2 Enforced  
> **Quality Gates**: `(G0–G4 + G-Review + G-Borrow + G-Law)`  

---

## 1. Spec Kit Specification

### 1.1 User Story Narrative
**As a** payment backend engineer operating under unreliable networks and at-least-once worker delivery,  
**I want to** have `given_id` UUIDs injected directly into mutating JSON request bodies (not HTTP headers), a compile-time-validated `Metadata` map, and a `PaginatedList<T>` with `Meta` cursors,  
**So that** retried requests never double-charge a customer, metadata never violates Moyasar's 30-key/40-char/500-char limits at runtime, and large result sets stream page-by-page without loading everything into memory.

#### 1.1.1 Preconditions
- The core microkernel is compiled (`features = ["core"]`).
- `uuid`, `serde_json`, `chrono` are available as core dependencies.

#### 1.1.2 Postconditions
- Every mutating payment request body contains a `given_id` UUID; the same UUID becomes the resulting `Payment.id`.
- Retrying an identical payload with the same `given_id` returns the original response without re-charging.
- Reusing a `given_id` with a *different* payload surfaces HTTP 400 "Payment is already created."
- `Metadata::insert` rejects: >30 keys, key >40 chars, value >500 chars.
- An empty-string metadata value unsets that key on update payloads.
- `PaginatedList<T>` deserializes `{ "<resource>": [...], "meta": {...} }` and follows `next_page`.

### 1.2 Acceptance Criteria (Gherkin Scenarios)

```gherkin
Feature: Idempotency, Metadata Validation and Pagination

  Scenario: [AC-US-1.2.0-01] Automatic UUIDv4 given_id injection into the request body
    Given a CreatePaymentParams builder with no explicitly supplied given_id
    When the request body is serialized
    Then the JSON body contains a "given_id" field holding a valid UUIDv4
    And no "Idempotency-Key" HTTP header is emitted (Moyasar does not use header idempotency).

  Scenario: [AC-US-1.2.0-02] Explicit given_id is preserved verbatim and becomes the Payment id
    Given a CreatePaymentParams with given_id "3fa85f64-5717-4562-b3fc-2c963f66afa6"
    When the payment is created successfully
    Then the response Payment.id equals "3fa85f64-5717-4562-b3fc-2c963f66afa6".

  Scenario: [AC-US-1.2.0-03] Deterministic UUIDv5 derivation for recurring subscription cycles
    Given a subscription id "sub_42" and cycle index 7
    When GivenIdPolicy::Deterministic derives the idempotency key
    Then the result equals Uuid::new_v5(&NAMESPACE, "sub_42:7")
    And deriving it again with identical inputs yields the exact same UUID.

  Scenario: [AC-US-1.2.0-04] Metadata limit enforcement at insert time
    Given a Metadata map containing 30 keys
    When a 31st key is inserted
    Then the call returns Err(MoyasarError::MetadataTooManyKeys)
    And inserting a key longer than 40 characters returns Err(MetadataKeyTooLong)
    And inserting a value longer than 500 characters returns Err(MetadataValueTooLong).

  Scenario: [AC-US-1.2.0-05] Empty-string metadata value unsets the key
    Given a Metadata map containing "order_id" -> "ORD-1"
    When "order_id" is set to an empty string and serialized into an update payload
    Then the JSON body contains "metadata": { "order_id": "" }
    And Moyasar is instructed to unset that key per its documented semantics.

  Scenario: [AC-US-1.2.0-06] PaginatedList deserialization and next-page traversal
    Given a list response with meta.total_pages = 3 and meta.current_page = 1
    When the response is deserialized into PaginatedList<Payment>
    Then items contains the payments for page 1 and meta.next_page equals 2.

  Scenario: [AC-US-1.2.0-LAW] Regulatory traceability of idempotent transaction records
    Given any mutating payment operation
    When the operation is executed or retried
    Then the given_id provides an immutable, auditable correlation key linking request to Payment.id,
         satisfying SAMA audit-trail requirements for financial transaction reconciliation.
```

### 1.3 Test Cases & Traceability Matrix

| Test Case ID | Target Criterion | Test Type | Execution Level | Verification Assertion & Pass Criteria |
| :--- | :--- | :--- | :--- | :--- |
| `TC-1.2.0-01` | `AC-US-1.2.0-01` | Functional | Unit | Serialized body JSON contains `given_id` parseable as UUID v4. |
| `TC-1.2.0-02` | `AC-US-1.2.0-02` | Functional | Integration | Mock returns the posted `given_id` as `id`; assertion on equality. |
| `TC-1.2.0-03` | `AC-US-1.2.0-03` | Functional | Unit | Repeated derivation is bit-identical; matches `Uuid::new_v5` golden vector. |
| `TC-1.2.0-04` | `AC-US-1.2.0-04` | Validation | Unit | Boundary tests at 30/31 keys, 40/41 chars, 500/501 chars. |
| `TC-1.2.0-05` | `AC-US-1.2.0-05` | Functional | Unit | Empty-string value serializes as `""` and is preserved. |
| `TC-1.2.0-06` | `AC-US-1.2.0-06` | Functional | Unit | Fixture JSON with 3 pages deserializes with correct `meta` fields. |
| `TC-1.2.0-LAW` | `AC-US-1.2.0-LAW` | Compliance | Static / Audit | Audit log correlation key present on every mutating operation. |

---

## 2. Behavior-Centric Architecture & Folder Blueprint

### 2.1 Rust Crate Module & Unit File Ownership Tree

```
src/
├── src/kernel/idempotency.rs                  # Unit 1 (feat/us-1.2.0/unit-1-given-id-policy)
├── src/kernel/idempotency_v5.rs               # Unit 2 (feat/us-1.2.0/unit-2-uuidv5-hashing)
├── src/kernel/metadata.rs                     # Unit 3 (feat/us-1.2.0/unit-3-metadata)
├── src/kernel/pagination.rs                   # Unit 4 (feat/us-1.2.0/unit-4-pagination-types)
├── src/kernel/paginator_stream.rs             # Unit 5 (feat/us-1.2.0/unit-5-pagination-stream)
```

### 2.2 Scoping & Promotion Blueprint
- **Tier 1 (Behaviour-Local)**: `GivenIdPolicy` private namespace constant and UUID derivation.
- **Tier 2 (Module-Shared)**: `Metadata` and `Meta` used by every resource service.
- **Tier 3 (Feature-Shared)**: `PaginatedList<T>` returned by all `list()` methods.
- **Tier 4 (Global-Shared)**: Exposed through `prelude::*`.

---

## 3. Technical Implementation Plan

### 3.1 Domain Invariants & Storage Schema
- **Body-Idempotency Law**: `given_id` lives in the JSON body. Any `Idempotency-Key` header emission is a bug.
- **Determinism Law**: `Uuid::new_v5(&NAMESPACE, "{sub_id}:{cycle_index}")` must be stable across processes and restarts.
- **Validation-at-Insert Law**: Metadata constraints are enforced on `insert()`, never deferred to serialization.
- **Retry Safety Law**: Automatic retries are enabled *only* for network errors, timeouts, and 5xx — never for 4xx.

```rust
pub enum GivenIdPolicy {
    Random,                                    // UUIDv4 per operation
    Deterministic { namespace: Uuid, scope: String },
}

impl GivenIdPolicy {
    pub fn derive(&self) -> Uuid {
        match self {
            Self::Random => Uuid::new_v4(),
            Self::Deterministic { namespace, scope } => Uuid::new_v5(namespace, scope.as_bytes()),
        }
    }
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Metadata(BTreeMap<String, String>);

impl std::fmt::Debug for Metadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Metadata").field("keys_count", &self.0.len()).finish()
    }
}

impl Metadata {
    pub fn insert(&mut self, key: &str, value: &str) -> Result<(), MoyasarError> {
        if self.0.len() >= METADATA_MAX_KEYS && !self.0.contains_key(key) {
            return Err(MoyasarError::MetadataTooManyKeys);
        }
        if key.chars().count() > METADATA_KEY_MAX_LEN {
            return Err(MoyasarError::MetadataKeyTooLong);
        }
        if value.chars().count() > METADATA_VALUE_MAX_LEN {
            return Err(MoyasarError::MetadataValueTooLong);
        }
        self.0.insert(key.to_owned(), value.to_owned());
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Meta {
    pub current_page: u32,
    pub next_page: Option<u32>,
    pub prev_page: Option<u32>,
    pub total_pages: u32,
    pub total_count: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PaginatedList<T> {
    #[serde(
        alias = "payments",
        alias = "invoices",
        alias = "payouts",
        alias = "payout_accounts",
        alias = "settlements",
        alias = "transfers",
        alias = "lines",
        alias = "internal_transactions",
        alias = "webhooks",
        alias = "attempts"
    )]
    pub items: Vec<T>,
    pub meta: Meta,
}
```

### 3.2 Activity & Interaction Diagram

```mermaid
sequenceDiagram
    autonumber
    actor Worker as Background Worker
    participant Policy as GivenIdPolicy
    participant Params as CreatePaymentParams
    participant Client as MoyasarClient
    participant API as api.moyasar.com

    Worker->>Policy: derive() -> UUIDv5("sub_42:7")
    Policy-->>Worker: given_id = 8f3a...
    Worker->>Params: .given_id(given_id)
    Worker->>Client: payments().create(params)
    Client->>API: POST /v1/payments { given_id: "8f3a...", ... }
    alt network timeout / 5xx
        Worker->>Client: retry with SAME given_id
        Client->>API: POST /v1/payments { given_id: "8f3a...", ... }
        API-->>Client: 201 with ORIGINAL Payment (no double charge)
    else given_id reused with different payload
        API-->>Client: 400 "Payment is already created."
    end
```

---

## 4. Granular Implementation Tasks

### 4.1 Dynamic 5-Unit Parallel Implementation Plan

| Unit # | Branch | Exclusive Owner Files | Deliverables & Colocated Unit Tests | Accountable Role / Agent | Assigned Skills |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Unit 1** | `feat/us-1.2.0/unit-1-given-id-policy` | `src/kernel/idempotency.rs` | `GivenIdPolicy` enum (Random/Deterministic), UUIDv4 generation, `given_id` injection into JSON bodies | `engineering-software-architect` | `security-audit` |
| **Unit 2** | `feat/us-1.2.0/unit-2-uuidv5-hashing` | `src/kernel/idempotency_v5.rs` | `Uuid::new_v5(&NAMESPACE, "{sub_id}:{cycle_index}")` deterministic derivation for subscription billing, roundtrip tests | `engineering-backend-architect` | `security-audit` |
| **Unit 3** | `feat/us-1.2.0/unit-3-metadata` | `src/kernel/metadata.rs` | `Metadata` map: `insert`/`remove`/`get`, validation on key count/length/value, empty-string key unset, unit tests for boundary conditions | `engineering-backend-architect` | `security-audit` |
| **Unit 4** | `feat/us-1.2.0/unit-4-pagination-types` | `src/kernel/pagination.rs` | `PaginatedList<T>`, `Meta` struct (current_page, next_page, prev_page, total_pages, total_count), `Deserialize` | `security-appsec-engineer` | `security-audit` |
| **Unit 5** | `feat/us-1.2.0/unit-5-pagination-stream` | `src/kernel/paginator_stream.rs` | `Paginator<T>::into_stream()` with `futures::Stream`, auto-page following next_page, unit tests covering 1-page and multi-page responses | `engineering-rust-refactoring-specialist` | `security-audit` |

### 4.2 Parallel Unit Execution & Story Merge Breakdown

#### Wave 1: Base Branch Scaffolding (`feat/us-1.2.0-*`)
- [x] **Task 0.1 (Story Base Contracts)**: Verify `Cargo.toml`, `src/lib.rs`, and the domain's `mod.rs` + `service.rs` facade declarations are present on the story base branch before spawning parallel unit worktrees.  
  *Agent*: `story-leader` | *Skill*: `divide-and-conquer-orchestrator`

#### Wave 2: 100% Parallel Unit Implementation (`Unit 1..5`)
- [x] **Unit 1 (`feat/us-1.2.0/unit-1-given-id-policy`)**: `GivenIdPolicy` enum (Random/Deterministic), UUIDv4 generation, `given_id` injection into JSON bodies (Files: `src/kernel/idempotency.rs`)  
  *Agent*: `engineering-software-architect` | *Skill*: `security-audit`
- [x] **Unit 2 (`feat/us-1.2.0/unit-2-uuidv5-hashing`)**: `Uuid::new_v5(&NAMESPACE, "{sub_id}:{cycle_index}")` deterministic derivation for subscription billing, roundtrip tests (Files: `src/kernel/idempotency_v5.rs`)  
  *Agent*: `engineering-backend-architect` | *Skill*: `security-audit`
- [x] **Unit 3 (`feat/us-1.2.0/unit-3-metadata`)**: `Metadata` map: `insert`/`remove`/`get`, validation on key count/length/value, empty-string key unset, unit tests for boundary conditions (Files: `src/kernel/metadata.rs`)  
  *Agent*: `engineering-backend-architect` | *Skill*: `security-audit`
- [x] **Unit 4 (`feat/us-1.2.0/unit-4-pagination-types`)**: `PaginatedList<T>`, `Meta` struct (current_page, next_page, prev_page, total_pages, total_count), `Deserialize` (Files: `src/kernel/pagination.rs`)  
  *Agent*: `security-appsec-engineer` | *Skill*: `security-audit`
- [x] **Unit 5 (`feat/us-1.2.0/unit-5-pagination-stream`)**: `Paginator<T>::into_stream()` with `futures::Stream`, auto-page following next_page, unit tests covering 1-page and multi-page responses (Files: `src/kernel/paginator_stream.rs`)  
  *Agent*: `engineering-rust-refactoring-specialist` | *Skill*: `security-audit`

#### Wave 3: Cascading Merge & Story Integration Verification
- [x] **Task 3.1 (Cascading Merge & Wire Verification)**: Merge `Unit 1 -> ... -> Unit 5` into the story branch and run story-level wire integration tests and `G0–G4` quality gates.  
  *Agent*: `testing-api-tester` | *Skill*: `security-audit`
- [x] **Task 3.2 (Council Review & Sign-Off)**: Execute independent Multi-Council Review (`G-Review`, `G-Borrow`, `G-Complexity`, `G-Docs`, `G-Law`).  
  *Agent*: `engineering-code-reviewer` | *Skill*: `code-review-council`

---

## 5. Security Gateway (Powered by security-audit)

### 5.1 Trust Boundaries & Security Actors
- **Trust Boundaries**:
  - `Idempotency Boundary`: the `given_id` namespace — must be unguessable to prevent cross-tenant collisions.
  - `Metadata Boundary`: merchant-controlled key/value space injected into financial records.
- **Authorized Principals**: any holder of a valid `SecretKey`.

### 5.2 Threat Modeling & Abuse Matrix (STRIDE / ASVS)

| Threat Vector | Severity | Vulnerability Mechanism | Mitigating Control & Security Invariant |
| :--- | :--- | :--- | :--- |
| **Replay / Double Charge** | Critical | Retry after timeout re-executes the charge | Body-injected `given_id`; Moyasar returns the original response. |
| **Idempotency Collision** | High | Reusing a `given_id` with a different payload | Server returns HTTP 400 `"Payment is already created."` — surfaced as typed error. |
| **Injection via Metadata** | High | Malicious metadata keys polluting audit records | Length caps + charset validation; no key interpolation into URLs. |
| **Predictable Subscription Keys** | Medium | Weak UUIDv5 namespace allows key guessing | Fixed private namespace constant; scope string includes the subscription id. |
| **Unbounded Memory** | Medium | `list()` loading all pages at once | `PaginatedList` is page-bounded; streaming is opt-in. |

### 5.3 Cryptographic & Data Protection Controls
- **UUIDv5 Namespace**: a fixed, non-secret, documented namespace constant — uniqueness, not secrecy, is the goal.
- **Retry Envelope**: retries only on network errors, timeouts, 5xx; never on 4xx.
- **Metadata Sanitization**: length and count enforced before serialization.

### 5.4 Automated Security Verification Checklist
- [x] **Retry Safety**: integration test proves no double charge after a simulated timeout.
- [x] **Collision Test**: same `given_id` + different payload yields a typed 400 error.
- [x] **Boundary Tests**: metadata limits enforced at insert, not deferred.
- [x] **No Header Idempotency**: assertion that no `Idempotency-Key` header is ever emitted.

### 5.5 Security Gateway (G2) Sign-Off Checklist
- [x] Retry envelope verified against OWASP ASVS Level 2.
- [x] Idempotency collision handling surfaces a typed, non-retryable error.
- [x] Metadata constraints enforced pre-serialization.
- [x] Independent reviewer sign-off; zero open findings.

---

## 6. Validation Gates & Acceptance Sign-Off

**Status: Implemented & Verified (Merged to `main`).**

### G0 — Build, lint & format
- [x] **US-1.2.0-G0:** `cargo build && cargo clippy -- -D warnings && cargo fmt --check` clean.

### G1 — Test coverage
- [x] **US-1.2.0-G1:** ≥90% line / ≥85% branch on idempotency, metadata, pagination modules.

### G2 — Security audit
- [x] **US-1.2.0-G2:** Retry-replay audit passes with zero critical/high findings.

### G3 — Architecture and maintainability
- [x] **US-1.2.0-G3:** Idempotency/metadata/pagination remain kernel-only; zero resource coupling.

### G-Review — Code review
- [x] **US-1.2.0-G-Review:** Peer review on determinism and retry envelope.

### G-Borrow — Rust zero-clone validation
- [x] **US-1.2.0-G-Borrow:** No needless `String` clones in metadata insertion paths.

### G-Complexity — Cyclomatic complexity
- [x] **US-1.2.0-G-Complexity:** $CC \le 6$; nesting $\le 2$.

### G-Docs — Public docs & examples
- [x] **US-1.2.0-G-Docs:** 100% public symbols documented; subscription UUIDv5 example is runnable.

### G-Law — Law validation
- [x] **US-1.2.0-G-Law:** SAMA audit-trail correlation via `given_id → Payment.id` certified.

### G4 — Integrated acceptance
- [x] **US-1.2.0-G4:** Retry-after-timeout returns the identical Payment with a single charge.
