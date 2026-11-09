# US-2.5.0: Marketplace Payment Splits & Account Funding Transactions

> **Spec Kit Specification**: [Specification Lifecycle Active](../../../docs/architecture/DECOMPOSITION_ROADMAP.md)  
> **Behavior Blueprint**: `src/resources/payments/`  
> **Slicing Standard**: [10-PR Vertical Slicing Framework](../README.md#2-the-10-pr-vertical-slicing-framework-rust-sdk-adaptation)  
> **Security Gateway**: `security-audit` Level 2 Enforced  
> **Quality Gates**: `(G0–G4 + G-Review + G-Borrow + G-Law)`  

---

## 1. Spec Kit Specification

### 1.1 User Story Narrative
**As a** marketplace platform operator routing funds between vendors and my own commission account,  
**I want to** attach a `splits` array to payment creation with typed recipient roles (`Entity`, `Platform`, `Beneficiary`), fee-source designation, and refundability flags, plus an AFT `sender` object for account funding transactions,  
**So that** a single customer charge can be divided across multiple beneficiaries at capture time, processing fees land on the correct party, and split reversals follow the parent payment's refund.

#### 1.1.1 Preconditions
- `features = ["payments"]`; valid `SecretKey`.
- Recipient UUIDs exist as Moyasar entities/platforms/beneficiaries.
- For AFT, sender identification fields are available.

#### 1.1.2 Postconditions
- `splits` serializes as an array of `PaymentSplitRequest` with `amount`, `recipient_type`, `recipient_id`, optional `reference`/`description`, `fee_source`, `refundable`.
- Split amounts are validated: each non-zero, and the sum does not exceed the payment `amount`.
- `fee_source: true` designates which split absorbs Moyasar's processing fee.
- `refundable: true` (default) means the split is reversed when the parent payment is refunded.
- AFT `sender` carries `first_name`, `last_name`, and identification keys.

### 1.2 Acceptance Criteria (Gherkin Scenarios)

```gherkin
Feature: Marketplace Splits and Account Funding Transactions

  Scenario: [AC-US-2.5.0-01] Two-way split serialization with typed recipient roles
    Given a payment of 15000 with splits of 13500 to a Beneficiary and 1500 to the Platform
    When the payment is created
    Then the body contains a splits array of length 2
    And the first entry has recipient_type "beneficiary" and the second "platform".

  Scenario: [AC-US-2.5.0-02] Split sum exceeding the payment amount is rejected
    Given a payment of 10000 with splits totalling 12000
    When the params are validated
    Then the call returns Err(MoyasarError::SplitTotalExceedsAmount)
    And splits totalling exactly 10000 are accepted.

  Scenario: [AC-US-2.5.0-03] Zero or negative split amounts are rejected
    Given a split with amount 0
    When the params are validated
    Then the call returns Err(MoyasarError::InvalidSplitAmount).

  Scenario: [AC-US-2.5.0-04] fee_source designates the fee-absorbing party
    Given a Platform split with fee_source true and a Beneficiary split with fee_source false
    When the payment is created
    Then the serialized Platform split contains "fee_source": true
    And the Beneficiary split contains "fee_source": false.

  Scenario: [AC-US-2.5.0-05] refundable defaults to true when omitted
    Given a split with no explicit refundable flag
    When serialized
    Then the field is either omitted or serialized as true
    And the server treats the split as refundable.

  Scenario: [AC-US-2.5.0-06] AFT sender object is attached to the payment
    Given a payment with an AFT sender containing first_name and last_name
    When the payment is created
    Then the body contains a sender object with those fields
    And the payment is created successfully.

  Scenario: [AC-US-2.5.0-LAW] SAMA marketplace split transparency and auditability
    Given any payment carrying splits
    When the payment is created
    Then each split's recipient_id, amount and reference are persisted for reconciliation,
         satisfying SAMA marketplace-funds-flow transparency requirements.
```

### 1.3 Test Cases & Traceability Matrix

| Test Case ID | Target Criterion | Test Type | Execution Level | Verification Assertion & Pass Criteria |
| :--- | :--- | :--- | :--- | :--- |
| `TC-2.5.0-01` | `AC-US-2.5.0-01` | Functional | Unit | Serialized array length 2; recipient_type strings exact. |
| `TC-2.5.0-02` | `AC-US-2.5.0-02` | Validation | Unit | 12000 > 10000 rejected; 10000 == 10000 accepted. |
| `TC-2.5.0-03` | `AC-US-2.5.0-03` | Validation | Unit | Zero and negative amounts rejected. |
| `TC-2.5.0-04` | `AC-US-2.5.0-04` | Functional | Unit | `fee_source` serialized per split. |
| `TC-2.5.0-05` | `AC-US-2.5.0-05` | Functional | Unit | Omitted `refundable` defaults to true. |
| `TC-2.5.0-06` | `AC-US-2.5.0-06` | Functional | Integration | Sender object present in body; payment created. |
| `TC-2.5.0-LAW` | `AC-US-2.5.0-LAW` | Compliance | Static / Audit | Split recipient/amount/reference persisted. |

---

## 2. Behavior-Centric Architecture & Folder Blueprint

### 2.1 Rust Crate Module & Unit File Ownership Tree

```
src/
├── src/resources/payments/splits.rs           # Unit 1 (feat/us-2.5.0/unit-1-split-types)
├── src/resources/payments/split_validation.rs # Unit 2 (feat/us-2.5.0/unit-2-split-validation)
├── src/resources/payments/aft.rs              # Unit 3 (feat/us-2.5.0/unit-3-aft-sender)
├── src/resources/payments/splits_response.rs  # Unit 4 (feat/us-2.5.0/unit-4-split-response-helpers)
```

### 2.2 Scoping & Promotion Blueprint
- **Tier 1 (Behaviour-Local)**: split-sum validator.
- **Tier 2 (Module-Shared)**: `PaymentSplitRequest` used by payments and stripe-compat refunds.
- **Tier 3 (Feature-Shared)**: `SplitRecipientType` used by settlements reconciliation.
- **Tier 4 (Global-Shared)**: exported via `prelude::*`.

---

## 3. Technical Implementation Plan

### 3.1 Domain Invariants & Storage Schema
- **Sum Law**: `sum(split.amount) <= payment.amount`; each split `amount > 0`.
- **Fee Law**: exactly one split may carry `fee_source: true`; if none, the platform absorbs the fee.
- **Refund Cascade Law**: `refundable: true` splits reverse proportionally on parent refund.
- **Reference Law**: `reference` is persisted into settlement files — it must be stable and unique per split.

```rust
#[derive(Serialize, Clone)]
pub struct PaymentSplitRequest {
    pub amount: u64,
    pub recipient_type: SplitRecipientType,
    pub recipient_id: Uuid,
    pub reference: Option<String>,
    pub description: Option<String>,
    pub fee_source: Option<bool>,
    pub refundable: Option<bool>,
}

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum SplitRecipientType { Entity, Platform, Beneficiary }

pub fn validate_splits(splits: &[PaymentSplitRequest], total: u64) -> Result<(), MoyasarError> {
    if splits.iter().any(|s| s.amount == 0) {
        return Err(MoyasarError::InvalidSplitAmount);
    }
    if splits.iter().filter(|s| s.fee_source == Some(true)).count() > 1 {
        return Err(MoyasarError::MultipleFeeSources);
    }
    let sum = splits.iter().try_fold(0u64, |acc, s| acc.checked_add(s.amount))
        .ok_or(MoyasarError::SplitTotalExceedsAmount)?;
    if sum > total {
        return Err(MoyasarError::SplitTotalExceedsAmount);
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AftSender {
    pub first_name: String,
    pub last_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nationality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_number: Option<String>,
}
pub type AfTSender = AftSender;
```

### 3.2 Activity & Interaction Diagram

```mermaid
sequenceDiagram
    autonumber
    actor Platform as Marketplace Backend
    participant Val as SplitValidator
    participant Svc as PaymentsService
    participant API as api.moyasar.com

    Platform->>Val: validate_splits([13500 Beneficiary, 1500 Platform], 15000)
    Val-->>Platform: Ok(())
    Platform->>Svc: create(params with splits)
    Svc->>API: POST /v1/payments { amount, splits: [...] }
    API-->>Svc: Payment { status: paid, splits: [...] }
    Note over API: On refund, refundable splits reverse proportionally
```

---

## 4. Granular Implementation Tasks

### 4.1 Dynamic 4-Unit Parallel Implementation Plan

| Unit # | Branch | Exclusive Owner Files | Deliverables & Colocated Unit Tests | Accountable Role / Agent | Assigned Skills |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Unit 1** | `feat/us-2.5.0/unit-1-split-types` | `src/resources/payments/splits.rs` | `PaymentSplitRequest`, `SplitRecipientType` enum (`Entity`, `Platform`, `Beneficiary`), `fee_source`/`refundable` defaults | `engineering-software-architect` | `security-audit` |
| **Unit 2** | `feat/us-2.5.0/unit-2-split-validation` | `src/resources/payments/split_validation.rs` | `validate_splits` with `checked_add` sum guard (`sum <= total`), zero-amount guard, single `fee_source` check, unit tests | `engineering-backend-architect` | `security-audit` |
| **Unit 3** | `feat/us-2.5.0/unit-3-aft-sender` | `src/resources/payments/aft.rs` | `AftSender` struct (`first_name`, `last_name`, optional identification fields) and `AfTSender` alias | `engineering-backend-architect` | `security-audit` |
| **Unit 4** | `feat/us-2.5.0/unit-4-split-response-helpers` | `src/resources/payments/splits_response.rs` | `PaymentSplitResponse` struct, proportional refund cascade calculator, and 2-way split + AFT unit tests | `security-appsec-engineer` | `security-audit` |

### 4.2 Parallel Unit Execution & Story Merge Breakdown

#### Wave 1: Base Branch Scaffolding (`feat/us-2.5.0-*`)
- [ ] **Task 0.1 (Story Base Contracts)**: Verify `Cargo.toml`, `src/lib.rs`, and the domain's `mod.rs` + `service.rs` facade declarations are present on the story base branch before spawning parallel unit worktrees.  
  *Agent*: `story-leader` | *Skill*: `divide-and-conquer-orchestrator`

#### Wave 2: 100% Parallel Unit Implementation (`Unit 1..4`)
- [ ] **Unit 1 (`feat/us-2.5.0/unit-1-split-types`)**: `PaymentSplitRequest`, `SplitRecipientType` enum (`Entity`, `Platform`, `Beneficiary`), `fee_source`/`refundable` defaults (Files: `src/resources/payments/splits.rs`)  
  *Agent*: `engineering-software-architect` | *Skill*: `security-audit`
- [ ] **Unit 2 (`feat/us-2.5.0/unit-2-split-validation`)**: `validate_splits` with `checked_add` sum guard (`sum <= total`), zero-amount guard, single `fee_source` check, unit tests (Files: `src/resources/payments/split_validation.rs`)  
  *Agent*: `engineering-backend-architect` | *Skill*: `security-audit`
- [ ] **Unit 3 (`feat/us-2.5.0/unit-3-aft-sender`)**: `AftSender` struct (`first_name`, `last_name`, optional identification fields) and `AfTSender` alias (Files: `src/resources/payments/aft.rs`)  
  *Agent*: `engineering-backend-architect` | *Skill*: `security-audit`
- [ ] **Unit 4 (`feat/us-2.5.0/unit-4-split-response-helpers`)**: `PaymentSplitResponse` struct, proportional refund cascade calculator, and 2-way split + AFT unit tests (Files: `src/resources/payments/splits_response.rs`)  
  *Agent*: `security-appsec-engineer` | *Skill*: `security-audit`

#### Wave 3: Cascading Merge & Story Integration Verification
- [ ] **Task 3.1 (Cascading Merge & Wire Verification)**: Merge `Unit 1 -> ... -> Unit 4` into the story branch and run story-level wire integration tests and `G0–G4` quality gates.  
  *Agent*: `testing-api-tester` | *Skill*: `security-audit`
- [ ] **Task 3.2 (Council Review & Sign-Off)**: Execute independent Multi-Council Review (`G-Review`, `G-Borrow`, `G-Complexity`, `G-Docs`, `G-Law`).  
  *Agent*: `engineering-code-reviewer` | *Skill*: `code-review-council`

---

## 5. Security Gateway (Powered by security-audit)

### 5.1 Trust Boundaries & Security Actors
- **Trust Boundaries**:
  - `Fund-Routing Boundary`: splits move money to third-party recipients.
  - `Recipient Boundary`: `recipient_id` must belong to the merchant's authorized network.
- **Authorized Principals**: marketplace backend holding a `SecretKey`.

### 5.2 Threat Modeling & Abuse Matrix (STRIDE / ASVS)

| Threat Vector | Severity | Vulnerability Mechanism | Mitigating Control & Security Invariant |
| :--- | :--- | :--- | :--- |
| **Fund Diversion** | Critical | Split recipient_id pointing to an unauthorized account | Server-side recipient authorization; SDK surfaces 4xx as typed errors. |
| **Over-Allocation** | Critical | Splits summing above the payment amount | Client-side sum guard before transport. |
| **Fee Misattribution** | High | Multiple `fee_source: true` splits | Exactly-one rule enforced; default to platform. |
| **Refund Leakage** | High | Non-refundable split reversed anyway | `refundable` flag respected by the refund cascade. |
| **Reference Collision** | Medium | Duplicate split references breaking reconciliation | Reference uniqueness documented and validated per payment. |

### 5.3 Cryptographic & Data Protection Controls
- **Sum Guard**: client-side validation before transmission.
- **Recipient Authorization**: server-enforced; SDK never assumes authority.
- **Audit Fields**: `reference` persisted into settlement files.

### 5.4 Automated Security Verification Checklist
- [ ] **Sum Test**: over-allocation rejected.
- [ ] **Zero Test**: zero/negative amounts rejected.
- [ ] **Fee Test**: multiple `fee_source` flagged.
- [ ] **Refund Cascade Test**: non-refundable splits preserved.

### 5.5 Security Gateway (G2) Sign-Off Checklist
- [ ] Fund-routing verified against OWASP ASVS Level 2.
- [ ] Over-allocation impossible client-side.
- [ ] Audit references persisted.
- [ ] Independent reviewer sign-off; zero open findings.

---

## 6. Validation Gates & Acceptance Sign-Off

**Status: Ready for Implementation.**

### G0 — Build, lint & format
- [ ] **US-2.5.0-G0:** `cargo build && cargo clippy -- -D warnings && cargo fmt --check` clean.

### G1 — Test coverage
- [ ] **US-2.5.0-G1:** ≥90% line / ≥85% branch on splits and AFT modules.

### G2 — Security audit
- [ ] **US-2.5.0-G2:** Fund-routing audit passes with zero critical/high findings.

### G3 — Architecture and maintainability
- [ ] **US-2.5.0-G3:** Split logic is pure validation; zero transport coupling.

### G-Review — Code review
- [ ] **US-2.5.0-G-Review:** Peer review on split arithmetic and refund cascade.

### G-Borrow — Rust zero-clone validation
- [ ] **US-2.5.0-G-Borrow:** Validation borrows the slice; no split cloning.

### G-Complexity — Cyclomatic complexity
- [ ] **US-2.5.0-G-Complexity:** $CC \le 6$; nesting $\le 2$.

### G-Docs — Public docs & examples
- [ ] **US-2.5.0-G-Docs:** 100% public symbols documented; two-way split example runnable.

### G-Law — Law validation
- [ ] **US-2.5.0-G-Law:** SAMA marketplace funds-flow transparency certified.

### G4 — Integrated acceptance
- [ ] **US-2.5.0-G4:** Two-way split and AFT sender verified against the mock server.
