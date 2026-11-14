# US-3.1.0: Payment Intent Creation, Retrieval, Cancellation & Expiry Guards

> **Spec Kit Specification**: [Specification Lifecycle Active](../../../docs/architecture/DECOMPOSITION_ROADMAP.md)  
> **Behavior Blueprint**: `src/resources/payment_intents/`  
> **Slicing Standard**: [10-PR Vertical Slicing Framework](../README.md#2-the-10-pr-vertical-slicing-framework-rust-sdk-adaptation)  
> **Security Gateway**: `security-audit` Level 2 Enforced  
> **Quality Gates**: `(G0–G4 + G-Review + G-Borrow + G-Law)`  

---

## 1. Spec Kit Specification

### 1.1 User Story Narrative
**As a** backend engineer building a tamper-proof browser/mobile checkout,  
**I want to** create a `PaymentIntent` on the server that locks `amount`, `currency`, `description`, and `callback_url`, then retrieve or cancel it — with an `is_usable()` guard that accounts for Moyasar's fixed, non-configurable 1-hour expiry window where status still reads `"created"`,  
**So that** my frontend can collect card details with only a `pk_` key and a `client_secret`, my backend never issues a cancel that returns `intent_not_cancelable`, and I never treat a `fulfilled` intent as a successful payment.

#### 1.1.1 Preconditions
- `features = ["payment-intents"]` (implies `core`, `payments`).
- A valid `SecretKey` for create/retrieve/cancel.

#### 1.1.2 Postconditions
- `POST /v1/payment_intents` returns a `PaymentIntent` with `status: "created"`, a `client_secret`, and `expires_at = created_at + 1 hour`.
- `GET /v1/payment_intents/:id` returns the intent including `client_secret`; calling it with a `pk_` returns `403 Forbidden`.
- `POST /v1/payment_intents/:id/cancel` returns `status: "canceled"`, or `400 intent_not_cancelable` if already fulfilled/canceled/expired.
- `PaymentIntent::is_usable()` returns true only when `status == Created && Utc::now() < expires_at`.

### 1.2 Acceptance Criteria (Gherkin Scenarios)

```gherkin
Feature: Payment Intent Lifecycle Management

  Scenario: [AC-US-3.1.0-01] Create an intent that locks the four immutable parameters
    Given a CreatePaymentIntentParams with amount 25000, currency SAR and a callback_url
    When client.payment_intents().create(params) is called with a SecretKey
    Then the response status equals "created"
    And client_secret is present and non-empty
    And expires_at equals created_at plus one hour.

  Scenario: [AC-US-3.1.0-02] Retrieve an intent by id
    Given an existing payment intent id
    When client.payment_intents().retrieve(id) is called with a SecretKey
    Then the returned intent includes client_secret
    And a 404 maps to MoyasarError::Api { error_type: RecordNotFound }.

  Scenario: [AC-US-3.1.0-03] Publishable key cannot retrieve an intent
    Given a MoyasarClient authenticated with a PublishableKey
    When code attempts client.payment_intents().retrieve(id)
    Then the program fails to compile
    And the equivalent runtime call returns 403 Forbidden per Moyasar's key-type rules.

  Scenario: [AC-US-3.1.0-04] Cancel a usable intent
    Given an intent with status "created" whose expires_at is in the future
    When client.payment_intents().cancel(id) is called
    Then the response status equals "canceled".

  Scenario: [AC-US-3.1.0-05] is_usable guards against the silent expiry window
    Given an intent with status "created" but expires_at one minute in the past
    When PaymentIntent::is_usable() is evaluated
    Then it returns false
    And a subsequent cancel() would return 400 intent_not_cancelable.

  Scenario: [AC-US-3.1.0-06] Cancel on a non-cancelable intent surfaces a typed error
    Given an intent already in status "fulfilled"
    When cancel(id) is called
    Then the result is Err(MoyasarError::Api { error_type: IntentNotCancelable })
    And no retry is attempted because 400 is non-retryable.

  Scenario: [AC-US-3.1.0-LAW] SAMA checkout integrity via locked server-side parameters
    Given any intent created by the backend
    When the frontend fulfills it
    Then amount, currency, description and callback_url cannot be altered by the client,
         satisfying SAMA requirements for tamper-proof transaction origination.
```

### 1.3 Test Cases & Traceability Matrix

| Test Case ID | Target Criterion | Test Type | Execution Level | Verification Assertion & Pass Criteria |
| :--- | :--- | :--- | :--- | :--- |
| `TC-3.1.0-01` | `AC-US-3.1.0-01` | Functional | Integration | Status `created`; `client_secret` present; `expires_at - created_at == 3600s`. |
| `TC-3.1.0-02` | `AC-US-3.1.0-02` | Functional | Integration | Retrieve includes `client_secret`; 404 → `RecordNotFound`. |
| `TC-3.1.0-03` | `AC-US-3.1.0-03` | Type Safety | Compile-fail (`trybuild`) | `pk_` + `retrieve()` fails to compile. |
| `TC-3.1.0-04` | `AC-US-3.1.0-04` | Functional | Integration | Cancel returns `canceled`. |
| `TC-3.1.0-05` | `AC-US-3.1.0-05` | State Machine | Unit | `is_usable()` false when expired despite status `created`. |
| `TC-3.1.0-06` | `AC-US-3.1.0-06` | Error Handling | Unit | 400 maps to `IntentNotCancelable`; `is_retryable()` false. |
| `TC-3.1.0-LAW` | `AC-US-3.1.0-LAW` | Compliance | Static / Audit | Locked parameters enforced server-side. |

---

## 2. Behavior-Centric Architecture & Folder Blueprint

### 2.1 Rust Crate Module & Unit File Ownership Tree

```
src/
├── src/resources/payment_intents/models.rs    # Unit 1 (feat/us-3.1.0/unit-1-intent-model)
├── src/resources/payment_intents/create.rs    # Unit 2 (feat/us-3.1.0/unit-2-create-cancel)
├── src/resources/payment_intents/expiry.rs    # Unit 3 (feat/us-3.1.0/unit-3-expiry-guard)
├── src/resources/payment_intents/retrieve.rs  # Unit 4 (feat/us-3.1.0/unit-4-retrieve-endpoint)
```

### 2.2 Scoping & Promotion Blueprint
- **Tier 1 (Behaviour-Local)**: expiry comparison logic.
- **Tier 2 (Module-Shared)**: `PaymentIntent` used by fulfillment (US-3.2.0).
- **Tier 3 (Feature-Shared)**: `PaymentIntentsService` under `payment-intents`.
- **Tier 4 (Global-Shared)**: `client.payment_intents()` facade accessor.

---

## 3. Technical Implementation Plan

### 3.1 Domain Invariants & Storage Schema
- **Expiry Law**: 1 hour, non-configurable. `INTENT_EXPIRY_SECS = 3600`.
- **Silent-Status Law**: an expired intent still reports `status: "created"`. Never trust status alone.
- **Fulfilled≠Paid Law**: `Fulfilled` means an attempt occurred, not that money moved.
- **Secret Law**: `client_secret` appears only in create/retrieve responses; `Debug` must redact it.

```rust
#[derive(Clone, Deserialize)]
pub struct PaymentIntent {
    pub id: Uuid,
    pub status: PaymentIntentStatus,
    pub amount: u64,
    pub amount_format: Option<String>,
    pub currency: Currency,
    pub description: Option<String>,
    pub callback_url: String,
    pub client_secret: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl std::fmt::Debug for PaymentIntent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaymentIntent")
            .field("id", &self.id)
            .field("status", &self.status)
            .field("amount", &self.amount)
            .field("currency", &self.currency)
            .field("client_secret", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl PaymentIntent {
    /// True only when the intent is still cancelable/fulfillable.
    pub fn is_usable(&self) -> bool {
        self.status == PaymentIntentStatus::Created && Utc::now() < self.expires_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaymentIntentStatus {
    Created, Fulfilled, Canceled,
    #[serde(other)]
    Unknown,
}
```

### 3.2 Activity & Interaction Diagram

```mermaid
sequenceDiagram
    autonumber
    actor Backend
    participant Svc as PaymentIntentsService
    participant Guard as ExpiryGuard
    participant API as api.moyasar.com

    Backend->>Svc: create(params) with SecretKey
    Svc->>API: POST /v1/payment_intents
    API-->>Svc: Intent { status: created, client_secret, expires_at }
    Svc-->>Backend: send (id, client_secret) to frontend
    Backend->>Svc: retrieve(id)
    Svc-->>Backend: Intent
    Backend->>Guard: is_usable()
    alt status == Created and not expired
        Guard-->>Backend: true
        Backend->>Svc: cancel(id)
        Svc->>API: POST /v1/payment_intents/:id/cancel
        API-->>Svc: Intent { status: canceled }
    else expired but status still "created"
        Guard-->>Backend: false
        Note over Backend: avoid spurious 400 intent_not_cancelable
    end
```

---

## 4. Granular Implementation Tasks

### 4.1 Dynamic 4-Unit Parallel Implementation Plan

| Unit # | Branch | Exclusive Owner Files | Deliverables & Colocated Unit Tests | Accountable Role / Agent | Assigned Skills |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Unit 1** | `feat/us-3.1.0/unit-1-intent-model` | `src/resources/payment_intents/models.rs` | `PaymentIntent` struct (id, amount, currency, description, callback_url, status, client_secret, expires_at, created_at), `PaymentIntentStatus` enum | `engineering-software-architect` | `security-audit` |
| **Unit 2** | `feat/us-3.1.0/unit-2-create-cancel` | `src/resources/payment_intents/create.rs` | `CreatePaymentIntentParams`, `client.payment_intents().create()`, `client.payment_intents().cancel(id)` returning 400 on already cancelled/fulfilled/expired | `engineering-backend-architect` | `security-audit` |
| **Unit 3** | `feat/us-3.1.0/unit-3-expiry-guard` | `src/resources/payment_intents/expiry.rs` | `PaymentIntent::is_usable(&self) -> bool` — status==Created AND Utc::now() < expires_at. Document pitfalls: status stays "created" past expiry | `engineering-backend-architect` | `security-audit` |
| **Unit 4** | `feat/us-3.1.0/unit-4-retrieve-endpoint` | `src/resources/payment_intents/retrieve.rs` | `client.payment_intents().retrieve(id)` (`SecretKey` bound), `IntentId` newtype, 403/404 handling, unit tests | `security-appsec-engineer` | `security-audit` |

### 4.2 Parallel Unit Execution & Story Merge Breakdown

#### Wave 1: Base Branch Scaffolding (`feat/us-3.1.0-*`)
- [x] **Task 0.1 (Story Base Contracts)**: Verify `Cargo.toml`, `src/lib.rs`, and the domain's `mod.rs` + `service.rs` facade declarations are present on the story base branch before spawning parallel unit worktrees.  
  *Agent*: `story-leader` | *Skill*: `divide-and-conquer-orchestrator`

#### Wave 2: 100% Parallel Unit Implementation (`Unit 1..4`)
- [x] **Unit 1 (`feat/us-3.1.0/unit-1-intent-model`)**: `PaymentIntent` struct (id, amount, currency, description, callback_url, status, client_secret, expires_at, created_at), `PaymentIntentStatus` enum (Files: `src/resources/payment_intents/models.rs`)  
  *Agent*: `engineering-software-architect` | *Skill*: `security-audit`
- [x] **Unit 2 (`feat/us-3.1.0/unit-2-create-cancel`)**: `CreatePaymentIntentParams`, `client.payment_intents().create()`, `client.payment_intents().cancel(id)` returning 400 on already cancelled/fulfilled/expired (Files: `src/resources/payment_intents/create.rs`)  
  *Agent*: `engineering-backend-architect` | *Skill*: `security-audit`
- [x] **Unit 3 (`feat/us-3.1.0/unit-3-expiry-guard`)**: `PaymentIntent::is_usable(&self) -> bool` — status==Created AND Utc::now() < expires_at. Document pitfalls: status stays "created" past expiry (Files: `src/resources/payment_intents/expiry.rs`)  
  *Agent*: `engineering-backend-architect` | *Skill*: `security-audit`
- [x] **Unit 4 (`feat/us-3.1.0/unit-4-retrieve-endpoint`)**: `client.payment_intents().retrieve(id)` (`SecretKey` bound), `IntentId` newtype, 403/404 handling, unit tests (Files: `src/resources/payment_intents/retrieve.rs`)  
  *Agent*: `security-appsec-engineer` | *Skill*: `security-audit`

#### Wave 3: Cascading Merge & Story Integration Verification
- [x] **Task 3.1 (Cascading Merge & Wire Verification)**: Merge `Unit 1 -> ... -> Unit 4` into the story branch and run story-level wire integration tests and `G0–G4` quality gates.  
  *Agent*: `testing-api-tester` | *Skill*: `security-audit`
- [x] **Task 3.2 (Council Review & Sign-Off)**: Execute independent Multi-Council Review (`G-Review`, `G-Borrow`, `G-Complexity`, `G-Docs`, `G-Law`).  
  *Agent*: `engineering-code-reviewer` | *Skill*: `code-review-council`

---

## 5. Security Gateway (Powered by security-audit)

### 5.1 Trust Boundaries & Security Actors
- **Trust Boundaries**:
  - `Client-Secret Boundary`: `client_secret` authorizes frontend fulfillment.
  - `Key-Type Boundary`: `pk_` may fulfill; only `sk_` may create/retrieve/cancel.
- **Authorized Principals**: backend (`sk_`), storefront (`pk_` + `client_secret`).

### 5.2 Threat Modeling & Abuse Matrix (STRIDE / ASVS)

| Threat Vector | Severity | Vulnerability Mechanism | Mitigating Control & Security Invariant |
| :--- | :--- | :--- | :--- |
| **Amount Tampering** | Critical | Client altering amount/currency at fulfillment | Server locks the four parameters; overrides ignored. |
| **client_secret Leakage** | Critical | Secret logged or echoed in errors | `skip_serializing` + redacted `Debug`. |
| **Privilege Escalation** | Critical | `pk_` retrieving intents or client_secrets | Compile-time `SecretKey` bound on retrieve. |
| **Spurious 400s** | Medium | Canceling an expired intent | `is_usable()` guard before any cancel. |
| **False Settlement** | High | Treating `fulfilled` as `paid` | Documented explicitly; Payment status must be checked separately. |

### 5.3 Cryptographic & Data Protection Controls
- **Secret Redaction**: `client_secret` never serialized into logs.
- **Key-Type Enforcement**: `SecretKey` bound on create/retrieve/cancel.
- **Expiry Guard**: prevents wasted API calls and misleading state.

### 5.4 Automated Security Verification Checklist
- [x] **client_secret Test**: absent from `Debug` output.
- [x] **Key-Type Test**: `pk_` cannot create/retrieve/cancel.
- [x] **Expiry Test**: boundary at exactly 3600 seconds.
- [x] **Non-Retry Test**: `intent_not_cancelable` is never retried.

### 5.5 Security Gateway (G2) Sign-Off Checklist
- [x] Checkout integrity verified against OWASP ASVS Level 2.
- [x] Zero `client_secret` in logs or telemetry.
- [x] Parameter locking verified server-side.
- [x] Independent reviewer sign-off; zero open findings.

---

## 6. Validation Gates & Acceptance Sign-Off

**Status: Implemented & Verified (Merged to `main`).**

### G0 — Build, lint & format
- [x] **US-3.1.0-G0:** `cargo build && cargo clippy -- -D warnings && cargo fmt --check` clean.

### G1 — Test coverage
- [x] **US-3.1.0-G1:** ≥90% line / ≥85% branch on the intent lifecycle module.

### G2 — Security audit
- [x] **US-3.1.0-G2:** Client-secret and key-type audit passes with zero critical/high findings.

### G3 — Architecture and maintainability
- [x] **US-3.1.0-G3:** Intent module depends only on kernel + payments; zero cyclical coupling.

### G-Review — Code review
- [x] **US-3.1.0-G-Review:** Peer review on expiry semantics and status handling.

### G-Borrow — Rust zero-clone validation
- [x] **US-3.1.0-G-Borrow:** `is_usable()` takes `&self`; zero allocation.

### G-Complexity — Cyclomatic complexity
- [x] **US-3.1.0-G-Complexity:** $CC \le 6$; nesting $\le 2$.

### G-Docs — Public docs & examples
- [x] **US-3.1.0-G-Docs:** 100% public symbols documented; expiry pitfall documented.

### G-Law — Law validation
- [x] **US-3.1.0-G-Law:** SAMA tamper-proof origination certified.

### G4 — Integrated acceptance
- [x] **US-3.1.0-G4:** Create→retrieve→cancel cycle verified against the mock server.
