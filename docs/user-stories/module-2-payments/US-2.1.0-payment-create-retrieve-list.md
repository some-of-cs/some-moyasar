# US-2.1.0: Payment Creation, Retrieval & Filtered Listing

> **Spec Kit Specification**: [Specification Lifecycle Active](../../../docs/architecture/DECOMPOSITION_ROADMAP.md)  
> **Behavior Blueprint**: `src/resources/payments/`  
> **Slicing Standard**: [10-PR Vertical Slicing Framework](../README.md#2-the-10-pr-vertical-slicing-framework-rust-sdk-adaptation)  
> **Security Gateway**: `security-audit` Level 2 Enforced  
> **Quality Gates**: `(G0–G4 + G-Review + G-Borrow + G-Law)`  

---

## 1. Spec Kit Specification

### 1.1 User Story Narrative
**As a** Saudi e-commerce backend engineer,  
**I want to** create, retrieve, and list payments through `client.payments()` with a builder that enforces Moyasar's required fields and body-injected `given_id`,  
**So that** I can accept Mada/STC Pay/credit card checkouts, poll payment state for reconciliation, and page through transaction history with server-side filters.

#### 1.1.1 Preconditions
- `some-moyasar` compiled with `features = ["payments"]` (implies `core`).
- A valid `SecretKey` for retrieve/list; `SecretKey` **or** `PublishableKey` for create.
- For source-based creation, a valid `PaymentSourceRequest` (see US-2.3.0 / US-2.4.0).

#### 1.1.2 Postconditions
- `POST /v1/payments` returns a `Payment` with `status` in `{initiated, paid, authorized, failed, verified, expired}`.
- `Payment` carries both integer minor-unit fields (`amount`, `fee`, `refunded`, `captured`) and pre-formatted strings (`amount_format`, `fee_format`, …).
- `GET /v1/payments/:id` returns the authoritative current state.
- `GET /v1/payments` returns `PaginatedList<Payment>` filtered by `page`, `id`, `status`, `created[gt|lt]`, `metadata[key]`.

### 1.2 Acceptance Criteria (Gherkin Scenarios)

```gherkin
Feature: Payment Creation, Retrieval and Listing

  Scenario: [AC-US-2.1.0-01] Create a payment with the minimal required fields
    Given a CreatePaymentParams with amount 15000, currency SAR, and a valid token source
    When client.payments().create(params) is called with a SecretKey
    Then the API receives POST /v1/payments with a body containing amount, currency and given_id
    And the response deserializes into a Payment whose amount equals 15000.

  Scenario: [AC-US-2.1.0-02] Payment response exposes both minor units and formatted strings
    Given a Payment fixture with amount 15000 and amount_format "150.00"
    When the Payment is deserialized
    Then payment.amount equals 15000 and payment.amount_format equals "150.00"
    And fee, refunded and captured follow the same dual representation.

  Scenario: [AC-US-2.1.0-03] Retrieve by id returns the authoritative state
    Given an existing payment id
    When client.payments().retrieve(id) is called
    Then the response Payment.id equals the requested id
    And a 404 response maps to MoyasarError::Api with ApiErrorType::RecordNotFound.

  Scenario: [AC-US-2.1.0-04] List with server-side filters and pagination
    Given a list request specifying status "paid", created[gt] and page 2
    When client.payments().list(params) is called
    Then the query string contains status=paid, created[gt]=<ts> and page=2
    And the response deserializes into PaginatedList<Payment> with meta.current_page == 2.

  Scenario: [AC-US-2.1.0-05] Metadata bracket-notation filter serialization
    Given a list request filtered by metadata["order_id"] with value "ORD-1"
    When the query string is serialized
    Then it contains the bracketed key metadata[order_id]=ORD-1.

  Scenario: [AC-US-2.1.0-LAW] Publishable-key isolation on create vs retrieve
    Given a MoyasarClient authenticated with a PublishableKey
    When code attempts client.payments().retrieve(id)
    Then the program fails to compile
    And create() remains available to satisfy the 4 documented pk_ origination routes.
```

### 1.3 Test Cases & Traceability Matrix

| Test Case ID | Target Criterion | Test Type | Execution Level | Verification Assertion & Pass Criteria |
| :--- | :--- | :--- | :--- | :--- |
| `TC-2.1.0-01` | `AC-US-2.1.0-01` | Functional | Integration | Mock asserts POST path, body contains `amount`, `currency`, `given_id`; Payment deserializes. |
| `TC-2.1.0-02` | `AC-US-2.1.0-02` | Functional | Unit | Dual-field deserialization test across amount/fee/refunded/captured. |
| `TC-2.1.0-03` | `AC-US-2.1.0-03` | Functional | Integration | 200 → matching id; 404 → `RecordNotFound`. |
| `TC-2.1.0-04` | `AC-US-2.1.0-04` | Functional | Unit | Query string byte-comparison against golden vector. |
| `TC-2.1.0-05` | `AC-US-2.1.0-05` | Functional | Unit | Bracketed metadata key present in serialized query. |
| `TC-2.1.0-LAW` | `AC-US-2.1.0-LAW` | Type Safety | Compile-fail (`trybuild`) | `pk_` + `retrieve()` fails to compile. |

---

## 2. Behavior-Centric Architecture & Folder Blueprint

### 2.1 Rust Crate Module & Unit File Ownership Tree

```
src/
├── src/resources/payments/models.rs           # Unit 1 (feat/us-2.1.0/unit-1-payment-model)
├── src/resources/payments/params.rs           # Unit 2 (feat/us-2.1.0/unit-2-create-params)
├── src/resources/payments/create.rs           # Unit 3 (feat/us-2.1.0/unit-3-create-endpoint)
├── src/resources/payments/retrieve.rs         # Unit 4 (feat/us-2.1.0/unit-4-retrieve-endpoint)
├── src/resources/payments/list.rs             # Unit 5 (feat/us-2.1.0/unit-5-list-endpoint)
```

### 2.2 Scoping & Promotion Blueprint
- **Tier 1 (Behaviour-Local)**: query-string serializer for list filters.
- **Tier 2 (Module-Shared)**: `Payment` and `PaymentStatus` used by lifecycle, intents, webhooks.
- **Tier 3 (Feature-Shared)**: `PaymentsService` re-exported by the payments feature.
- **Tier 4 (Global-Shared)**: `client.payments()` on the root client facade.

---

## 3. Technical Implementation Plan

### 3.1 Domain Invariants & Storage Schema
- **Amount Law**: `amount` is always an integer in the currency's smallest unit (halalas for SAR).
- **State Law**: `PaymentStatus` is an exhaustive enum; unknown wire values deserialize to `Unknown(String)`.
- **Idempotency Law**: create() injects `given_id`; retrieve()/list() never do.
- **Bracket Notation Law**: metadata query filters use `metadata[key]=value`.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PaymentId(pub Uuid);

#[derive(Clone, Deserialize)]
pub struct Payment {
    pub id: Uuid,
    pub status: PaymentStatus,
    pub amount: u64,
    pub amount_format: String,
    pub fee: u64,
    pub fee_format: String,
    pub refunded: u64,
    pub refunded_format: String,
    pub captured: u64,
    pub captured_format: String,
    pub currency: Currency,
    pub description: Option<String>,
    pub source: PaymentSourceResponse,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    pub splits: Option<Vec<PaymentSplitResponse>>,
    pub callback_url: Option<String>,
    pub invoice_id: Option<Uuid>,
    pub ip: Option<String>,
    pub captured_at: Option<DateTime<Utc>>,
    pub refunded_at: Option<DateTime<Utc>>,
    pub voided_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl std::fmt::Debug for Payment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Payment")
            .field("id", &self.id)
            .field("status", &self.status)
            .field("amount", &self.amount)
            .field("currency", &self.currency)
            .field("metadata_keys", &self.metadata.keys().collect::<Vec<_>>())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaymentStatus {
    Initiated,
    Paid,
    Authorized,
    Captured,
    Voided,
    Refunded,
    Failed,
    Verified,
    Expired,
    #[serde(other)]
    Unknown,
}
```

### 3.2 Activity & Interaction Diagram

```mermaid
sequenceDiagram
    autonumber
    actor Merchant as Merchant Backend
    participant Svc as PaymentsService
    participant Param as CreatePaymentParams
    participant Client as MoyasarClient
    participant API as api.moyasar.com

    Merchant->>Param: builder().amount(15000).currency(SAR).source(token)
    Param-->>Merchant: validated params (given_id injected)
    Merchant->>Svc: client.payments().create(params)
    Svc->>Client: POST /v1/payments
    Client->>API: JSON body with given_id
    API-->>Client: 201 Payment { status: initiated }
    Client-->>Merchant: Payment
    Merchant->>Svc: client.payments().retrieve(id)
    Svc->>API: GET /v1/payments/:id
    API-->>Svc: Payment { status: paid }
```

---

## 4. Granular Implementation Tasks

### 4.1 Dynamic 5-Unit Parallel Implementation Plan

| Unit # | Branch | Exclusive Owner Files | Deliverables & Colocated Unit Tests | Accountable Role / Agent | Assigned Skills |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Unit 1** | `feat/us-2.1.0/unit-1-payment-model` | `src/resources/payments/models.rs` | `Payment` struct (all fields: id, amount, amount_format, fee, fee_format, status, source, metadata, splits, timestamps, callback_url, invoice_id, ip) | `engineering-software-architect` | `security-audit` |
| **Unit 2** | `feat/us-2.1.0/unit-2-create-params` | `src/resources/payments/params.rs` | `CreatePaymentParams::builder()` (amount, currency, description, callback_url, given_id, source, metadata, splits, sender), `Serialize` | `engineering-backend-architect` | `security-audit` |
| **Unit 3** | `feat/us-2.1.0/unit-3-create-endpoint` | `src/resources/payments/create.rs` | `client.payments().create(params)`, `given_id` injection, response deserialization, wire tests with mock | `engineering-backend-architect` | `security-audit` |
| **Unit 4** | `feat/us-2.1.0/unit-4-retrieve-endpoint` | `src/resources/payments/retrieve.rs` | `client.payments().retrieve(id)` / `.fetch(id)`, `PaymentId` newtype, 404 → record_not_found | `security-appsec-engineer` | `security-audit` |
| **Unit 5** | `feat/us-2.1.0/unit-5-list-endpoint` | `src/resources/payments/list.rs` | `client.payments().list(ListParams)`, query param serialization (page, id, status, created[gt/lt], metadata[key]), return `PaginatedList<Payment>` | `engineering-rust-refactoring-specialist` | `security-audit` |

### 4.2 Parallel Unit Execution & Story Merge Breakdown

#### Wave 1: Base Branch Scaffolding (`feat/us-2.1.0-*`)
- [ ] **Task 0.1 (Story Base Contracts)**: Verify `Cargo.toml`, `src/lib.rs`, and the domain's `mod.rs` + `service.rs` facade declarations are present on the story base branch before spawning parallel unit worktrees.  
  *Agent*: `story-leader` | *Skill*: `divide-and-conquer-orchestrator`

#### Wave 2: 100% Parallel Unit Implementation (`Unit 1..5`)
- [ ] **Unit 1 (`feat/us-2.1.0/unit-1-payment-model`)**: `Payment` struct (all fields: id, amount, amount_format, fee, fee_format, status, source, metadata, splits, timestamps, callback_url, invoice_id, ip) (Files: `src/resources/payments/models.rs`)  
  *Agent*: `engineering-software-architect` | *Skill*: `security-audit`
- [ ] **Unit 2 (`feat/us-2.1.0/unit-2-create-params`)**: `CreatePaymentParams::builder()` (amount, currency, description, callback_url, given_id, source, metadata, splits, sender), `Serialize` (Files: `src/resources/payments/params.rs`)  
  *Agent*: `engineering-backend-architect` | *Skill*: `security-audit`
- [ ] **Unit 3 (`feat/us-2.1.0/unit-3-create-endpoint`)**: `client.payments().create(params)`, `given_id` injection, response deserialization, wire tests with mock (Files: `src/resources/payments/create.rs`)  
  *Agent*: `engineering-backend-architect` | *Skill*: `security-audit`
- [ ] **Unit 4 (`feat/us-2.1.0/unit-4-retrieve-endpoint`)**: `client.payments().retrieve(id)` / `.fetch(id)`, `PaymentId` newtype, 404 → record_not_found (Files: `src/resources/payments/retrieve.rs`)  
  *Agent*: `security-appsec-engineer` | *Skill*: `security-audit`
- [ ] **Unit 5 (`feat/us-2.1.0/unit-5-list-endpoint`)**: `client.payments().list(ListParams)`, query param serialization (page, id, status, created[gt/lt], metadata[key]), return `PaginatedList<Payment>` (Files: `src/resources/payments/list.rs`)  
  *Agent*: `engineering-rust-refactoring-specialist` | *Skill*: `security-audit`

#### Wave 3: Cascading Merge & Story Integration Verification
- [ ] **Task 3.1 (Cascading Merge & Wire Verification)**: Merge `Unit 1 -> ... -> Unit 5` into the story branch and run story-level wire integration tests and `G0–G4` quality gates.  
  *Agent*: `testing-api-tester` | *Skill*: `security-audit`
- [ ] **Task 3.2 (Council Review & Sign-Off)**: Execute independent Multi-Council Review (`G-Review`, `G-Borrow`, `G-Complexity`, `G-Docs`, `G-Law`).  
  *Agent*: `engineering-code-reviewer` | *Skill*: `code-review-council`

---

## 5. Security Gateway (Powered by security-audit)

### 5.1 Trust Boundaries & Security Actors
- **Trust Boundaries**:
  - `Key-Type Boundary`: create accepts `pk_`; retrieve/list require `sk_`.
  - `Data Boundary`: `Payment` objects contain metadata and payer IP — PII-adjacent.
- **Authorized Principals**: merchant backend (`sk_`), merchant storefront (`pk_`).

### 5.2 Threat Modeling & Abuse Matrix (STRIDE / ASVS)

| Threat Vector | Severity | Vulnerability Mechanism | Mitigating Control & Security Invariant |
| :--- | :--- | :--- | :--- |
| **Privilege Escalation** | Critical | Publishable key reading payment history | Compile-time `SecretKey` bound on retrieve/list. |
| **Enumeration** | Medium | Sequential id guessing exposes other merchants' payments | UUIDv4 ids; authorization enforced server-side by key ownership. |
| **Log Leakage** | High | Full `Payment` serialized into logs includes metadata PII | `Debug` on `Payment` excludes `metadata` values by default. |
| **Query Injection** | High | Unsanitized metadata key breaking query structure | Keys percent-encoded; bracket notation escaped. |
| **Double Charge** | Critical | Retried create without idempotency | `given_id` mandatory on create. |

### 5.3 Cryptographic & Data Protection Controls
- **Identifier Entropy**: UUIDv4 payment ids, not sequential.
- **Key-Type Enforcement**: `SecretKey` bound on all read operations.
- **Query Encoding**: all filter values percent-encoded before transmission.

### 5.4 Automated Security Verification Checklist
- [ ] **Key-Type Test**: `trybuild` proves `pk_` cannot call `retrieve()`/`list()`.
- [ ] **PII Log Test**: `format!("{:?}", payment)` does not print metadata values.
- [ ] **Query Encoding Test**: special characters in metadata keys are escaped.
- [ ] **Idempotency Test**: create always emits `given_id`.

### 5.5 Security Gateway (G2) Sign-Off Checklist
- [ ] Trust boundaries verified against OWASP ASVS Level 2.
- [ ] Key-type isolation verified for all three endpoints.
- [ ] Zero PII in default `Debug` output.
- [ ] Independent reviewer sign-off; zero open findings.

---

## 6. Validation Gates & Acceptance Sign-Off

**Status: Ready for Implementation.**

### G0 — Build, lint & format
- [ ] **US-2.1.0-G0:** `cargo build && cargo clippy -- -D warnings && cargo fmt --check` clean.

### G1 — Test coverage
- [ ] **US-2.1.0-G1:** ≥90% line / ≥85% branch on the payments CRUD module.

### G2 — Security audit
- [ ] **US-2.1.0-G2:** Zero critical/high findings on key-type and PII handling.

### G3 — Architecture and maintainability
- [ ] **US-2.1.0-G3:** Resource module depends only on the kernel; zero cross-resource coupling.

### G-Review — Code review
- [ ] **US-2.1.0-G-Review:** Peer review by `code-reviewer`.

### G-Borrow — Rust zero-clone validation
- [ ] **US-2.1.0-G-Borrow:** `PaymentsService` borrows the client; zero deep clones.

### G-Complexity — Cyclomatic complexity
- [ ] **US-2.1.0-G-Complexity:** $CC \le 6$; nesting $\le 2$.

### G-Docs — Public docs & examples
- [ ] **US-2.1.0-G-Docs:** 100% public symbols documented; create→retrieve example runnable.

### G-Law — Law validation
- [ ] **US-2.1.0-G-Law:** PCI-DSS 4.0 data-minimization and SAMA transaction-record retention certified.

### G4 — Integrated acceptance
- [ ] **US-2.1.0-G4:** Create → retrieve → list round-trip succeeds against the mock server.
