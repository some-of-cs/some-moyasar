# US-6.1.0: Settlements Service — List, Retrieve, Lines & Balance

> **Spec Kit Specification**: [Specification Lifecycle Active](../../../docs/architecture/DECOMPOSITION_ROADMAP.md)  
> **Behavior Blueprint**: `features/core/modules/settlements/behaviours/settlements/`  
> **Slicing Standard**: [10-PR Vertical Slicing Framework](../README.md#2-the-10-pr-vertical-slicing-framework-rust-sdk-adaptation)  
> **Security Gateway**: `security-audit` Level 2 Enforced  
> **Quality Gates**: `(G0–G4 + G-Review + G-Borrow + G-Law)`  

---

## 1. Spec Kit Specification

### 1.1 User Story Narrative
**As a** finance engineer closing the books each day,  
**I want to** list and retrieve settlements, drill into their individual lines, and fetch the merchant balance through `client.settlements()`,  
**So that** I can reconcile gross volume against fees, tax and net settlement per currency, download the CSV/PDF reports Moyasar generates, and know exactly how much is available, pending and reserved.

#### 1.1.1 Preconditions
- `features = ["settlements"]` (implies `core`).
- A valid `SecretKey`.

#### 1.1.2 Postconditions
- `list()` returns `PaginatedList<Settlement>` filtered by `id`, `created[gt|lt]`, `page`.
- `retrieve(id)` returns a single `Settlement`.
- `list_lines(id)` returns `PaginatedList<SettlementLine>`.
- `fetch_balance()` returns `MerchantBalance` (available / pending / reserved).
- `Settlement` exposes `recipient_type`, `currency`, `source_currency`, `invoicing_currency`, `amount`, `fee`, `tax`, `invoicing_fee`, `invoicing_tax`, `invoicing_ex_rate`, `reference`, `settlement_count`, and the report URLs.

### 1.2 Acceptance Criteria (Gherkin Scenarios)

```gherkin
Feature: Settlement Reconciliation

  Scenario: [AC-US-6.1.0-01] List settlements within a date range
    Given a list request with created[gt] and created[lt] filters
    When client.settlements().list(params) is called
    Then the query string contains the encoded date filters
    And the response deserializes into PaginatedList<Settlement>.

  Scenario: [AC-US-6.1.0-02] Retrieve a settlement with monetary breakdown
    Given a settlement id
    When client.settlements().retrieve(id) is called
    Then amount, fee, tax and the invoicing_* fields are all populated
    And the relationship amount - fee - tax equals the net settled value.

  Scenario: [AC-US-6.1.0-03] Multi-currency settlement exposes invoicing FX rate
    Given a settlement with source_currency SAR and invoicing_currency USD
    When deserialized
    Then invoicing_ex_rate is a positive float
    And invoicing_fee and invoicing_tax are expressed in the invoicing currency.

  Scenario: [AC-US-6.1.0-04] Report URLs are present for download
    Given a settlement
    When deserialized
    Then invoice_url, csv_list_url and pdf_list_url are optional but, when present, are absolute URLs.

  Scenario: [AC-US-6.1.0-05] List settlement lines with pagination
    Given a settlement with multiple lines
    When client.settlements().list_lines(id, params) is called
    Then the response is a PaginatedList<SettlementLine>
    And each line references its originating payment.

  Scenario: [AC-US-6.1.0-06] Fetch merchant balance
    Given the merchant has settlement activity
    When client.settlements().fetch_balance() is called
    Then the response MerchantBalance reports available, pending and reserved amounts
    And each amount is expressed in the merchant's settlement currency.

  Scenario: [AC-US-6.1.0-LAW] SAMA reconciliation completeness and retention
    Given any settlement record
    When it is retrieved for audit
    Then gross amount, fee, tax, net amount and timestamps are all available for reconstruction,
         satisfying SAMA record-retention and reconciliation requirements.
```

### 1.3 Test Cases & Traceability Matrix

| Test Case ID | Target Criterion | Test Type | Execution Level | Verification Assertion & Pass Criteria |
| :--- | :--- | :--- | :--- | :--- |
| `TC-6.1.0-01` | `AC-US-6.1.0-01` | Functional | Unit | Encoded filters present; paginated shape correct. |
| `TC-6.1.0-02` | `AC-US-6.1.0-02` | Functional | Unit | `amount - fee - tax` arithmetic holds on fixtures. |
| `TC-6.1.0-03` | `AC-US-6.1.0-03` | Functional | Unit | `invoicing_ex_rate > 0`; invoicing fields populated. |
| `TC-6.1.0-04` | `AC-US-6.1.0-04` | Functional | Unit | URLs parse as absolute when present. |
| `TC-6.1.0-05` | `AC-US-6.1.0-05` | Functional | Integration | Lines paginate; line → payment linkage present. |
| `TC-6.1.0-06` | `AC-US-6.1.0-06` | Functional | Integration | Balance struct with three amounts. |
| `TC-6.1.0-LAW` | `AC-US-6.1.0-LAW` | Compliance | Static / Audit | Full breakdown reconstructable per settlement. |

---

## 2. Behavior-Centric Architecture & Folder Blueprint

### 2.1 Colocated Behaviour Directory Tree

```
features/core/modules/settlements/behaviours/settlements/
├── services/        # SettlementsService list/retrieve/lines/balance
├── logic/           # Date-filter serialization, net-amount derivation helpers
├── types/           # Settlement, SettlementLine, MerchantBalance, SettlementId
└── constants/       # SETTLEMENTS_PATH, BALANCE_PATH, LINES_SUFFIX
```

### 2.2 Scoping & Promotion Blueprint
- **Tier 1 (Behaviour-Local)**: net-amount helper.
- **Tier 2 (Module-Shared)**: `SettlementId` referenced by transfers.
- **Tier 3 (Feature-Shared)**: `SettlementsService` under `settlements`.
- **Tier 4 (Global-Shared)**: `client.settlements()` facade.

---

## 3. Technical Implementation Plan

### 3.1 Domain Invariants & Storage Schema
- **Read-Only Law**: every settlements endpoint is `GET`; no mutation methods exist.
- **Arithmetic Law**: `net = amount - fee - tax` in the settlement currency.
- **FX Law**: `invoicing_ex_rate` is a float, positive; invoicing amounts are in `invoicing_currency`.
- **Pagination Law**: all list endpoints return `PaginatedList<T>`.

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct Settlement {
    pub id: SettlementId,
    pub recipient_type: RecipientType,
    pub currency: Currency,
    pub source_currency: Currency,
    pub invoicing_currency: Option<Currency>,
    pub amount: u64,
    pub fee: u64,
    pub tax: u64,
    pub invoicing_fee: Option<u64>,
    pub invoicing_tax: Option<u64>,
    pub invoicing_ex_rate: Option<f64>,
    pub reference: Option<String>,
    pub settlement_count: Option<u32>,
    pub invoice_url: Option<String>,
    pub csv_list_url: Option<String>,
    pub pdf_list_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Settlement {
    /// Net settled amount in the settlement currency.
    pub fn net(&self) -> u64 { self.amount.saturating_sub(self.fee).saturating_sub(self.tax) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecipientType { Entity, Platform, Beneficiary }
```

### 3.2 Activity & Interaction Diagram

```mermaid
sequenceDiagram
    autonumber
    actor Finance
    participant Svc as SettlementsService
    participant API as api.moyasar.com

    Finance->>Svc: settlements().list({ created[gt], created[lt] })
    Svc->>API: GET /v1/settlements?created[gt]=...&created[lt]=...
    API-->>Svc: PaginatedList<Settlement>
    Finance->>Svc: settlements().retrieve(id)
    API-->>Svc: Settlement { amount, fee, tax, invoice_url }
    Finance->>Svc: settlements().list_lines(id)
    API-->>Svc: PaginatedList<SettlementLine>
    Finance->>Svc: settlements().fetch_balance()
    API-->>Svc: MerchantBalance { available, pending, reserved }
```

---

## 4. Granular Implementation Tasks

### 4.1 The 10-PR Vertical Slicing Implementation Plan

| PR # | Pillar | Architectural Layer | Primary Scope & Files | Deliverables & Acceptance | Accountable Role / Agent | Assigned Skills |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **PR 1** | **Pillar 1** | **Domain Logic & Core Traits** | `src/resources/settlements/models.rs` | `Settlement`, `SettlementLine`, `RecipientType`. | `engineering-software-architect` | `security-audit` |
| **PR 2** | **Pillar 2** | **Request Modeling** | `src/resources/settlements/params.rs` | List filter params with date bounds. | `engineering-backend-architect` | `security-audit` |
| **PR 3** | **Pillar 3** | **Transport Wire Adapter** | `src/resources/settlements/list.rs` | `GET /v1/settlements` with filters. | `engineering-backend-architect` | `security-audit` |
| **PR 4** | **Pillar 4** | **Service Accessor** | `src/resources/settlements/service.rs` | `client.settlements()` accessor. | `engineering-software-architect` | `security-audit` |
| **PR 5** | **Pillar 5** | **Error Taxonomy** | `src/kernel/error.rs` | 404 and date-parse error mapping. | `security-application-security-engineer` | `security-audit` |
| **PR 6** | **Pillar 6** | **Resource Facade Mounting** | `src/resources/settlements/mod.rs` | Feature-gated export under `settlements`. | `engineering-software-architect` | `security-audit` |
| **PR 7** | **Pillar 7** | **Zero-Copy Hardening** | `src/resources/settlements/models.rs` | `SettlementId` newtype; borrowed URLs. | `agency-rust-refactoring-specialist` | `security-audit` |
| **PR 8** | **Pillar 8** | **Edge Cases & Guards** | `src/resources/settlements/balance.rs` | Balance struct; missing FX fields tolerated. | `agency-api-tester` | `security-audit` |
| **PR 9** | **Pillar 9** | **Integration Wire Verification** | `tests/settlements_test.rs` | list/retrieve/lines/balance wire tests. | `agency-api-tester` | `security-audit` |
| **PR 10** | **Pillar 10** | **Documentation & Packaging** | `README.md` | Doc-tests with a reconciliation example. | `engineering-technical-writer` | `security-audit` |

### 4.2 Implementation Phase Breakdown

#### Phase 1: Models & Params (PR 1–2)
- [ ] **Task 1.1 (Types)**: `Settlement`, `SettlementLine`, `RecipientType`, `net()`.  
  *Agent*: `engineering-software-architect` | *Skill*: `security-audit`
- [ ] **Task 1.2 (Params)**: date-range filter params.  
  *Agent*: `engineering-backend-architect` | *Skill*: `security-audit`
- [ ] **Task 1.3 (Unit Tests)**: arithmetic and FX fixtures.  
  *Agent*: `agency-api-tester` | *Skill*: `security-audit`

#### Phase 2: Endpoints (PR 3–6)
- [ ] **Task 2.1 (List)**: `GET /v1/settlements` with encoded filters.  
  *Agent*: `engineering-backend-architect` | *Skill*: `security-audit`
- [ ] **Task 2.2 (Retrieve/Lines)**: `GET /:id` and `GET /:id/lines`.  
  *Agent*: `engineering-backend-architect` | *Skill*: `security-audit`
- [ ] **Task 2.3 (Balance)**: `GET /v1/settlements/balance`.  
  *Agent*: `engineering-software-architect` | *Skill*: `security-audit`

#### Phase 3: Edge Cases (PR 7–8)
- [ ] **Task 3.1 (Optional FX)**: tolerate absence of invoicing fields.  
  *Agent*: `agency-api-tester` | *Skill*: `security-audit`
- [ ] **Task 3.2 (URL Validation)**: absolute-URL checks when present.  
  *Agent*: `agency-api-tester` | *Skill*: `security-audit`

#### Phase 4: Verification & Sign-Off (PR 9–10)
- [ ] **Task 4.1 (Wire Tests)**: all four endpoints.  
  *Agent*: `agency-api-tester` | *Skill*: `security-audit`
- [ ] **Task 4.2 (G-Review)**: peer review on monetary arithmetic.  
  *Agent*: `agency-code-reviewer` | *Skill*: `security-audit`
- [ ] **Task 4.3 (Documentation)**: reconciliation walkthrough.  
  *Agent*: `engineering-technical-writer` | *Skill*: `security-audit`

---

## 5. Security Gateway (Powered by security-audit)

### 5.1 Trust Boundaries & Security Actors
- **Trust Boundaries**:
  - `Report Boundary`: report URLs grant access to financial documents.
  - `Data Boundary`: settlement data is merchant-confidential.
- **Authorized Principals**: merchant finance backend (`sk_`).

### 5.2 Threat Modeling & Abuse Matrix (STRIDE / ASVS)

| Threat Vector | Severity | Vulnerability Mechanism | Mitigating Control & Security Invariant |
| :--- | :--- | :--- | :--- |
| **Report URL Leakage** | High | Signed URLs logged or shared | URLs treated as confidential; redacted in `Debug`. |
| **Cross-Tenant Read** | Critical | Accessing another merchant's settlements | Server-side ownership; typed 404/403. |
| **Arithmetic Drift** | High | Float rounding causing reconciliation mismatch | Integer minor units with saturating arithmetic; floats only for FX rate. |
| **Date Filter Injection** | Medium | Malformed date strings breaking queries | Typed `DateTime<Utc>` params, encoded on the wire. |
| **Unbounded Listing** | Medium | Fetching all settlements at once | Mandatory pagination. |

### 5.3 Cryptographic & Data Protection Controls
- **Signed URLs**: treated as bearer credentials for documents.
- **Integer Money**: no floating-point arithmetic on amounts.
- **Typed Filters**: dates validated before serialization.

### 5.4 Automated Security Verification Checklist
- [ ] **Arithmetic Test**: `amount - fee - tax` verified on fixtures.
- [ ] **Tenant Test**: foreign id yields typed error.
- [ ] **URL Redaction Test**: `Debug` hides report URLs.
- [ ] **Filter Test**: malformed dates rejected before transport.

### 5.5 Security Gateway (G2) Sign-Off Checklist
- [ ] Report access verified against OWASP ASVS Level 2.
- [ ] Zero confidential data in logs.
- [ ] Monetary arithmetic certified.
- [ ] Independent reviewer sign-off; zero open findings.

---

## 6. Validation Gates & Acceptance Sign-Off

**Status: Ready for Implementation.**

### G0 — Build, lint & format
- [ ] **US-6.1.0-G0:** `cargo build && cargo clippy -- -D warnings && cargo fmt --check` clean.

### G1 — Test coverage
- [ ] **US-6.1.0-G1:** ≥90% line / ≥85% branch on the settlements module.

### G2 — Security audit
- [ ] **US-6.1.0-G2:** Financial-data audit passes with zero critical/high findings.

### G3 — Architecture and maintainability
- [ ] **US-6.1.0-G3:** Read-only module; zero mutation surface.

### G-Review — Code review
- [ ] **US-6.1.0-G-Review:** Peer review on arithmetic and filter handling.

### G-Borrow — Rust zero-clone validation
- [ ] **US-6.1.0-G-Borrow:** `SettlementId` and URL accessors avoid cloning.

### G-Complexity — Cyclomatic complexity
- [ ] **US-6.1.0-G-Complexity:** $CC \le 6$; nesting $\le 2$.

### G-Docs — Public docs & examples
- [ ] **US-6.1.0-G-Docs:** 100% public symbols documented; reconciliation example runnable.

### G-Law — Law validation
- [ ] **US-6.1.0-G-Law:** SAMA reconciliation completeness and retention certified.

### G4 — Integrated acceptance
- [ ] **US-6.1.0-G4:** List/retrieve/lines/balance verified against the mock server.
