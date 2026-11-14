# US-2.4.0: STC Pay OTP Flow & SADAD Bill Sources

> **Spec Kit Specification**: [Specification Lifecycle Active](../../../docs/architecture/DECOMPOSITION_ROADMAP.md)  
> **Behavior Blueprint**: `src/resources/payments/`  
> **Slicing Standard**: [10-PR Vertical Slicing Framework](../README.md#2-the-10-pr-vertical-slicing-framework-rust-sdk-adaptation)  
> **Security Gateway**: `security-audit` Level 2 Enforced  
> **Quality Gates**: `(G0–G4 + G-Review + G-Borrow + G-Law)`  

---

## 1. Spec Kit Specification

### 1.1 User Story Narrative
**As a** Saudi merchant accepting mobile-wallet and bank-bill payments,  
**I want to** initiate STC Pay charges that return a `transaction_url` for OTP completion, and create SADAD Bill payments that produce a `sadad_number` payable through the customer's banking app or ATM,  
**So that** I can serve customers who don't use cards, complete STC Pay charges with a second OTP call, and reconcile SADAD bills asynchronously via `payment_paid` / `payment_expired` webhooks.

#### 1.1.1 Preconditions
- `features = ["payments"]`; valid `SecretKey` (or `pk_` for STC Pay origination).
- STC Pay: customer's mobile number in `05xxxxxxxx`, `+9665xxxxxxxx`, or `009665xxxxxxxx` form.
- SADAD: optional `expired_at` no more than 29 days in the future.

#### 1.1.2 Postconditions
- STC Pay initiation returns `status: "initiated"` with `source.transaction_url` (`/v1/stc_pays/:id/proceed?otp_token=...`).
- `verify_stc_otp(url, otp)` posts `{"otp_value": "..."}` and transitions the payment to `paid` or `failed`.
- SADAD creation returns `sadad_number`, `biller_code`, `provider_name`, and `expired_at`.
- SADAD `expired_at` defaults to 29 days and is capped at the 29-day provider limit.

### 1.2 Acceptance Criteria (Gherkin Scenarios)

```gherkin
Feature: STC Pay and SADAD Bill Payment Sources

  Scenario: [AC-US-2.4.0-01] STC Pay mobile number normalization
    Given mobile inputs "0512345678", "+966512345678" and "00966512345678"
    When StcPaySource::new() validates each
    Then all three are accepted as the same underlying number
    And an input such as "12345" returns Err(MoyasarError::InvalidMobile).

  Scenario: [AC-US-2.4.0-02] STC Pay initiation returns a transaction_url
    Given a valid StcPaySource
    When the payment is created
    Then the response status is "initiated"
    And source.transaction_url contains "/v1/stc_pays/" and an "otp_token" query parameter.

  Scenario: [AC-US-2.4.0-03] OTP completion transitions the payment
    Given an initiated STC Pay payment with a transaction_url
    When client.payments().verify_stc_otp(url, "123456") is called
    Then the request body contains { "otp_value": "123456" }
    And the resulting Payment.status is "paid" or "failed".

  Scenario: [AC-US-2.4.0-04] SADAD bill creation returns a payable sadad_number
    Given a SadadBillSource with no expired_at
    When the payment is created
    Then the response source contains a numeric sadad_number
    And biller_code is one of 901, 902, or 007
    And expired_at is approximately 29 days after creation.

  Scenario: [AC-US-2.4.0-05] SADAD expiry capped at 29 days
    Given a SadadBillSource with expired_at set 45 days in the future
    When the source is validated
    Then the call returns Err(MoyasarError::SadadExpiryTooFar)
    And an expired_at 20 days out is accepted unchanged.

  Scenario: [AC-US-2.4.0-06] SADAD sandbox biller code 007 is accepted
    Given a SadadBillSource with biller_code "007" in test mode
    When the payment is created against the sandbox
    Then provider_name deserializes to the sandbox provider label
    And the payment is created successfully.

  Scenario: [AC-US-2.4.0-LAW] SAMA e-invoicing and bill-expiry consistency
    Given a SADAD bill linked to an invoice
    When the bill is created
    Then the bill's expired_at never exceeds the invoice expiry nor the 29-day provider cap,
         satisfying SAMA e-invoicing consistency requirements.
```

### 1.3 Test Cases & Traceability Matrix

| Test Case ID | Target Criterion | Test Type | Execution Level | Verification Assertion & Pass Criteria |
| :--- | :--- | :--- | :--- | :--- |
| `TC-2.4.0-01` | `AC-US-2.4.0-01` | Validation | Unit | Three accepted formats normalize identically; 5 invalid rejected. |
| `TC-2.4.0-02` | `AC-US-2.4.0-02` | Functional | Integration | Initiated response has `transaction_url` with `otp_token`. |
| `TC-2.4.0-03` | `AC-US-2.4.0-03` | Functional | Integration | OTP body exact; status transitions to paid/failed. |
| `TC-2.4.0-04` | `AC-US-2.4.0-04` | Functional | Integration | `sadad_number` numeric; `biller_code` in allowed set. |
| `TC-2.4.0-05` | `AC-US-2.4.0-05` | Validation | Unit | 45-day expiry rejected; 20-day accepted. |
| `TC-2.4.0-06` | `AC-US-2.4.0-06` | Functional | Integration | Sandbox biller 007 creates successfully. |
| `TC-2.4.0-LAW` | `AC-US-2.4.0-LAW` | Compliance | Static / Audit | Bill expiry ≤ invoice expiry and ≤ 29 days. |

---

## 2. Behavior-Centric Architecture & Folder Blueprint

### 2.1 Rust Crate Module & Unit File Ownership Tree

```
src/
├── src/resources/payments/source_stcpay.rs    # Unit 1 (feat/us-2.4.0/unit-1-stcpay-source)
├── src/resources/payments/stcpay_otp.rs       # Unit 2 (feat/us-2.4.0/unit-2-stcpay-otp)
├── src/resources/payments/source_sadad.rs     # Unit 3 (feat/us-2.4.0/unit-3-sadad-source)
├── src/resources/payments/sadad_expiry.rs     # Unit 4 (feat/us-2.4.0/unit-4-sadad-expiry-guard)
```

### 2.2 Scoping & Promotion Blueprint
- **Tier 1 (Behaviour-Local)**: mobile normalization and expiry capping helpers.
- **Tier 2 (Module-Shared)**: both source variants inside `PaymentSourceRequest`.
- **Tier 3 (Feature-Shared)**: `verify_stc_otp` on `PaymentsService`.
- **Tier 4 (Global-Shared)**: exported via `prelude::*`.

---

## 3. Technical Implementation Plan

### 3.1 Domain Invariants & Storage Schema
- **Mobile Law**: accept `05…`, `+9665…`, `009665…`; normalize to a canonical form before validation.
- **OTP Law**: `verify_stc_otp` posts to the **server-provided** `transaction_url`, never a constructed path.
- **SADAD Cap Law**: `expired_at` ≤ 29 days from creation; never exceeds the linked invoice expiry.
- **Async Law**: SADAD settlement is out-of-band; the SDK never polls — reconciliation is webhook-driven.

```rust
pub struct StcPaySource {
    pub mobile: String,
    pub cashier: Option<String>,
    pub branch: Option<String>,
}

impl StcPaySource {
    pub fn new(mobile: &str) -> Result<Self, MoyasarError> {
        let normalized = normalize_saudi_mobile(mobile)?;
        Ok(Self { mobile: normalized, cashier: None, branch: None })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SadadBillSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expired_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub biller_code: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SadadBillSourceResponse {
    pub provider_name: Option<String>,
    pub biller_code: String,
    pub sadad_number: String,
    pub gateway_id: Option<String>,
    pub expired_at: Option<DateTime<Utc>>,
    pub message: Option<String>,
}

impl SadadBillSource {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<(), MoyasarError> {
        if let Some(exp) = self.expired_at {
            if (exp - now).num_days() > SADAD_MAX_EXPIRY_DAYS {
                return Err(MoyasarError::SadadExpiryTooFar);
            }
        }
        Ok(())
    }
}
```

### 3.2 Activity & Interaction Diagram

```mermaid
sequenceDiagram
    autonumber
    actor Customer
    participant Svc as PaymentsService
    participant API as api.moyasar.com
    participant Bank as SADAD / STC Pay

    Customer->>Svc: create(StcPaySource { mobile })
    Svc->>API: POST /v1/payments { source: { type: "stcpay" } }
    API->>Bank: send SMS OTP
    API-->>Svc: Payment { status: initiated, transaction_url }
    Svc-->>Customer: prompt for OTP
    Customer->>Svc: verify_stc_otp(url, "123456")
    Svc->>API: POST {transaction_url} { otp_value }
    API-->>Svc: Payment { status: paid }
```

---

## 4. Granular Implementation Tasks

### 4.1 Dynamic 4-Unit Parallel Implementation Plan

| Unit # | Branch | Exclusive Owner Files | Deliverables & Colocated Unit Tests | Accountable Role / Agent | Assigned Skills |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Unit 1** | `feat/us-2.4.0/unit-1-stcpay-source` | `src/resources/payments/source_stcpay.rs` | `StcPaySource`: mobile (+966/05/00966 formats), cashier, branch. Mobile validation via regex | `engineering-software-architect` | `security-audit` |
| **Unit 2** | `feat/us-2.4.0/unit-2-stcpay-otp` | `src/resources/payments/stcpay_otp.rs` | `client.payments().verify_stc_otp(url, otp)`, POST to `transaction_url`, otp_value injection, status: initiated → paid/failed | `engineering-backend-architect` | `security-audit` |
| **Unit 3** | `feat/us-2.4.0/unit-3-sadad-source` | `src/resources/payments/source_sadad.rs` | `SadadBillSource` request (`expired_at`, optional sandbox `biller_code`) and `SadadBillSourceResponse` (`biller_code` 901/902/007, `sadad_number`, `provider_name`) | `engineering-backend-architect` | `security-audit` |
| **Unit 4** | `feat/us-2.4.0/unit-4-sadad-expiry-guard` | `src/resources/payments/sadad_expiry.rs` | SADAD 29-day maximum expiry validator, invoice-expiry cap helper, and sandbox unit tests (STC Pay 0515555555/58/59, SADAD biller 007) | `security-appsec-engineer` | `security-audit` |

### 4.2 Parallel Unit Execution & Story Merge Breakdown

#### Wave 1: Base Branch Scaffolding (`feat/us-2.4.0-*`)
- [x] **Task 0.1 (Story Base Contracts)**: Verify `Cargo.toml`, `src/lib.rs`, and the domain's `mod.rs` + `service.rs` facade declarations are present on the story base branch before spawning parallel unit worktrees.  
  *Agent*: `story-leader` | *Skill*: `divide-and-conquer-orchestrator`

#### Wave 2: 100% Parallel Unit Implementation (`Unit 1..4`)
- [x] **Unit 1 (`feat/us-2.4.0/unit-1-stcpay-source`)**: `StcPaySource`: mobile (+966/05/00966 formats), cashier, branch. Mobile validation via regex (Files: `src/resources/payments/source_stcpay.rs`)  
  *Agent*: `engineering-software-architect` | *Skill*: `security-audit`
- [x] **Unit 2 (`feat/us-2.4.0/unit-2-stcpay-otp`)**: `client.payments().verify_stc_otp(url, otp)`, POST to `transaction_url`, otp_value injection, status: initiated → paid/failed (Files: `src/resources/payments/stcpay_otp.rs`)  
  *Agent*: `engineering-backend-architect` | *Skill*: `security-audit`
- [x] **Unit 3 (`feat/us-2.4.0/unit-3-sadad-source`)**: `SadadBillSource` request (`expired_at`, optional sandbox `biller_code`) and `SadadBillSourceResponse` (`biller_code` 901/902/007, `sadad_number`, `provider_name`) (Files: `src/resources/payments/source_sadad.rs`)  
  *Agent*: `engineering-backend-architect` | *Skill*: `security-audit`
- [x] **Unit 4 (`feat/us-2.4.0/unit-4-sadad-expiry-guard`)**: SADAD 29-day maximum expiry validator, invoice-expiry cap helper, and sandbox unit tests (STC Pay 0515555555/58/59, SADAD biller 007) (Files: `src/resources/payments/sadad_expiry.rs`)  
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
  - `OTP Boundary`: the OTP is a bearer credential for completing a charge.
  - `Bill Boundary`: `sadad_number` is a payable reference exposed to the customer.
- **Authorized Principals**: storefront (`pk_`) for STC Pay origination; backend (`sk_`) for OTP completion.

### 5.2 Threat Modeling & Abuse Matrix (STRIDE / ASVS)

| Threat Vector | Severity | Vulnerability Mechanism | Mitigating Control & Security Invariant |
| :--- | :--- | :--- | :--- |
| **OTP Brute Force** | High | Repeated OTP attempts against a transaction | Server enforces attempt limits; SDK surfaces the 60-second cooldown error distinctly. |
| **OTP Leakage** | Critical | OTP logged in plaintext | OTP is a request-only field; never `Debug`-printed or persisted. |
| **URL Tampering** | High | Client constructs the OTP URL instead of using the server's | `verify_stc_otp` uses the server-provided `transaction_url` verbatim. |
| **Bill Expiry Abuse** | Medium | Bill outliving the invoice | 29-day cap plus invoice-expiry interaction enforced. |
| **Mobile Enumeration** | Low | Probing which numbers are registered | Sandbox numbers documented; production errors are generic. |

### 5.3 Cryptographic & Data Protection Controls
- **OTP Non-Retention**: OTP never stored or logged.
- **Server-Provided URL**: no client-side URL construction.
- **Expiry Capping**: single source of truth for the 29-day limit.

### 5.4 Automated Security Verification Checklist
- [x] **OTP Log Test**: OTP absent from all formatted output.
- [x] **URL Test**: `verify_stc_otp` uses the provided URL byte-for-byte.
- [x] **Expiry Test**: 29-day cap enforced.
- [x] **Cooldown Test**: 60-second cooldown error mapped distinctly.

### 5.5 Security Gateway (G2) Sign-Off Checklist
- [x] OTP handling verified against OWASP ASVS Level 2.
- [x] Zero OTP in logs or telemetry.
- [x] Expiry capping certified.
- [x] Independent reviewer sign-off; zero open findings.

---

## 6. Validation Gates & Acceptance Sign-Off

**Status: Implemented & Verified (Merged to `main`).**

### G0 — Build, lint & format
- [x] **US-2.4.0-G0:** `cargo build && cargo clippy -- -D warnings && cargo fmt --check` clean.

### G1 — Test coverage
- [x] **US-2.4.0-G1:** ≥90% line / ≥85% branch on STC Pay and SADAD modules.

### G2 — Security audit
- [x] **US-2.4.0-G2:** OTP-handling audit passes with zero critical/high findings.

### G3 — Architecture and maintainability
- [x] **US-2.4.0-G3:** Both sources are pure data + validation; zero transport coupling.

### G-Review — Code review
- [x] **US-2.4.0-G-Review:** Peer review on OTP flow and expiry logic.

### G-Borrow — Rust zero-clone validation
- [x] **US-2.4.0-G-Borrow:** Mobile normalization avoids intermediate allocations.

### G-Complexity — Cyclomatic complexity
- [x] **US-2.4.0-G-Complexity:** $CC \le 6$; nesting $\le 2$.

### G-Docs — Public docs & examples
- [x] **US-2.4.0-G-Docs:** 100% public symbols documented; OTP flow example runnable.

### G-Law — Law validation
- [x] **US-2.4.0-G-Law:** SAMA e-invoicing bill-expiry consistency certified.

### G4 — Integrated acceptance
- [x] **US-2.4.0-G4:** STC Pay OTP round-trip and SADAD creation verified against sandbox.
