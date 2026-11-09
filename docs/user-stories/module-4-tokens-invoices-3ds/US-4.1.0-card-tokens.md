# US-4.1.0: Card Tokenization Service — Create, Retrieve & Delete

> **Spec Kit Specification**: [Specification Lifecycle Active](../../../docs/architecture/DECOMPOSITION_ROADMAP.md)  
> **Behavior Blueprint**: `features/core/modules/tokens/behaviours/card-tokenization/`  
> **Slicing Standard**: [10-PR Vertical Slicing Framework](../README.md#2-the-10-pr-vertical-slicing-framework-rust-sdk-adaptation)  
> **Security Gateway**: `security-audit` Level 2 Enforced  
> **Quality Gates**: `(G0–G4 + G-Review + G-Borrow + G-Law)`  

---

## 1. Spec Kit Specification

### 1.1 User Story Narrative
**As a** subscription and card-on-file product engineer,  
**I want to** create, retrieve, and delete Moyasar card tokens through `client.tokens()`, with a token lifecycle of `initiated → active → inactive`,  
**So that** I can charge saved cards off-session later, display masked card details to customers, and revoke access when a customer removes their card — without ever storing a PAN.

#### 1.1.1 Preconditions
- `features = ["tokens"]` (implies `core`).
- A `PublishableKey` for `create()`; a `SecretKey` for `retrieve()` and `delete()`.

#### 1.1.2 Postconditions
- `POST /v1/tokens` returns a `Token` with `id` prefixed `token_` and `status: "initiated"`.
- After cardholder verification the token becomes `active`; it becomes `inactive` on verification failure or card expiry.
- `GET /v1/tokens/:id` returns masked card details suitable for customer display.
- `DELETE /v1/tokens/:id` returns HTTP `204 No Content` with an empty body.

### 1.2 Acceptance Criteria (Gherkin Scenarios)

```gherkin
Feature: Card Token Lifecycle

  Scenario: [AC-US-4.1.0-01] Create a token with a publishable key
    Given a CreateTokenParams with name, number, month, year and cvc
    When client.tokens().create(params) is called with a PublishableKey
    Then the response Token.id starts with "token_"
    And the response status equals "initiated".

  Scenario: [AC-US-4.1.0-02] save_only defers verification
    Given a CreateTokenParams with save_only true
    When the token is created
    Then the request body contains "save_only": true
    And the token remains "initiated" until cardholder verification completes.

  Scenario: [AC-US-4.1.0-03] Retrieve masked card details for display
    Given an active token id
    When client.tokens().retrieve(id) is called with a SecretKey
    Then the response contains a masked card number such as "XXXX-XXXX-XXXX-1111"
    And no full PAN is present anywhere in the Token struct.

  Scenario: [AC-US-4.1.0-04] Delete returns 204 with an empty body
    Given an existing token id
    When client.tokens().delete(id) is called with a SecretKey
    Then the API returns HTTP 204 No Content
    And the SDK returns Ok(()) rather than attempting to deserialize a body.

  Scenario: [AC-US-4.1.0-05] Token status transitions
    Given a token in status "initiated"
    When cardholder verification succeeds
    Then the token status becomes "active"
    And if verification fails or the card expires the status becomes "inactive".

  Scenario: [AC-US-4.1.0-LAW] PCI-DSS tokenization as PAN-substitution control
    Given any token creation
    When the token is persisted by the merchant
    Then only the "token_..." identifier and masked number are stored, never the PAN or CVC,
         satisfying PCI-DSS 4.0 Requirement 3.4 tokenization guidance.
```

### 1.3 Test Cases & Traceability Matrix

| Test Case ID | Target Criterion | Test Type | Execution Level | Verification Assertion & Pass Criteria |
| :--- | :--- | :--- | :--- | :--- |
| `TC-4.1.0-01` | `AC-US-4.1.0-01` | Functional | Integration | `id` starts with `token_`; status `initiated`. |
| `TC-4.1.0-02` | `AC-US-4.1.0-02` | Functional | Unit | `save_only` present in body when true. |
| `TC-4.1.0-03` | `AC-US-4.1.0-03` | Functional | Unit | Masked number matches `XXXX-XXXX-XXXX-\d{4}`; no 16-digit PAN. |
| `TC-4.1.0-04` | `AC-US-4.1.0-04` | Functional | Integration | 204 handled as `Ok(())` with empty-body deserialization. |
| `TC-4.1.0-05` | `AC-US-4.1.0-05` | State Machine | Unit | Status enum transitions mapped from fixtures. |
| `TC-4.1.0-LAW` | `AC-US-4.1.0-LAW` | Compliance | Static / Audit | No PAN/CVC retained in `Token`. |

---

## 2. Behavior-Centric Architecture & Folder Blueprint

### 2.1 Colocated Behaviour Directory Tree

```
features/core/modules/tokens/behaviours/card-tokenization/
├── services/        # TokensService create/retrieve/delete
├── logic/           # 204 empty-body handling, status normalization
├── types/           # Token, TokenStatus, TokenId, CreateTokenParams
└── constants/       # TOKENS_PATH, TOKEN_ID_PREFIX ("token_")
```

### 2.2 Scoping & Promotion Blueprint
- **Tier 1 (Behaviour-Local)**: 204 empty-body handler.
- **Tier 2 (Module-Shared)**: `TokenStatus` used by stripe-compat subscriptions.
- **Tier 3 (Feature-Shared)**: `TokensService` under `tokens`.
- **Tier 4 (Global-Shared)**: `client.tokens()` facade accessor.

---

## 3. Technical Implementation Plan

### 3.1 Domain Invariants & Storage Schema
- **Prefix Law**: token ids always start with `token_`; `TokenId(String)` enforces it at construction.
- **Empty-Body Law**: `DELETE` returns 204; the client must not attempt JSON deserialization.
- **Active-Before-Charge Law**: off-session charges require `TokenStatus::Active`.
- **PAN Non-Retention Law**: only masked numbers survive into the `Token` model.

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct Token {
    pub id: TokenId,
    pub status: TokenStatus,
    pub number: String,        // masked: XXXX-XXXX-XXXX-1111
    pub company: Option<String>,
    pub gateway_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TokenStatus { Initiated, Active, Inactive }

pub struct TokenId(String);

impl TokenId {
    pub fn new(raw: impl Into<String>) -> Result<Self, MoyasarError> {
        let raw = raw.into();
        if raw.starts_with(TOKEN_ID_PREFIX) { Ok(Self(raw)) }
        else { Err(MoyasarError::InvalidTokenId) }
    }
}

impl TokensService<'_> {
    pub async fn delete(&self, id: &TokenId) -> Result<(), MoyasarError> {
        // DELETE /v1/tokens/{id} -> 204 No Content (no body)
        self.client.delete_no_content(&format!("{TOKENS_PATH}/{id}")).await
    }
}
```

### 3.2 Activity & Interaction Diagram

```mermaid
sequenceDiagram
    autonumber
    actor Storefront
    participant Svc as TokensService
    participant API as api.moyasar.com
    actor Backend

    Storefront->>Svc: tokens().create(params) with pk_
    Svc->>API: POST /v1/tokens
    API-->>Svc: Token { id: token_..., status: initiated }
    Storefront->>API: cardholder verification
    API-->>Backend: token_paid webhook / verification
    Backend->>Svc: tokens().retrieve(id) with sk_
    Svc-->>Backend: Token { status: active, number: XXXX-...-1111 }
    Backend->>Svc: tokens().delete(id)
    Svc->>API: DELETE /v1/tokens/:id
    API-->>Svc: 204 No Content
```

---

## 4. Granular Implementation Tasks

### 4.1 The 10-PR Vertical Slicing Implementation Plan

| PR # | Pillar | Architectural Layer | Primary Scope & Files | Deliverables & Acceptance | Accountable Role / Agent | Assigned Skills |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **PR 1** | **Pillar 1** | **Domain Logic & Core Traits** | `src/resources/tokens/models.rs` | `Token`, `TokenStatus`, `TokenId` prefix guard. | `engineering-software-architect` | `security-audit` |
| **PR 2** | **Pillar 2** | **Request Modeling** | `src/resources/tokens/params.rs` | `CreateTokenParams` incl. `save_only`. | `engineering-backend-architect` | `security-audit` |
| **PR 3** | **Pillar 3** | **Transport Wire Adapter** | `src/resources/tokens/create.rs` | `POST /v1/tokens`. | `engineering-backend-architect` | `security-audit` |
| **PR 4** | **Pillar 4** | **Service Accessor** | `src/resources/tokens/service.rs` | `client.tokens()` accessor. | `engineering-software-architect` | `security-audit` |
| **PR 5** | **Pillar 5** | **Error Taxonomy** | `src/kernel/error.rs` | `InvalidTokenId`, 404 mapping. | `security-application-security-engineer` | `security-audit` |
| **PR 6** | **Pillar 6** | **Resource Facade Mounting** | `src/resources/tokens/mod.rs` | Feature-gated export under `tokens`. | `engineering-software-architect` | `security-audit` |
| **PR 7** | **Pillar 7** | **Zero-Copy Hardening** | `src/resources/tokens/models.rs` | `TokenId` borrow-friendly newtype. | `agency-rust-refactoring-specialist` | `security-audit` |
| **PR 8** | **Pillar 8** | **Edge Cases & Guards** | `src/resources/tokens/delete.rs` | 204 empty-body handling without deserialization. | `agency-api-tester` | `security-audit` |
| **PR 9** | **Pillar 9** | **Integration Wire Verification** | `tests/tokens_test.rs` | create→retrieve→delete round-trip; 204 handling. | `agency-api-tester` | `security-audit` |
| **PR 10** | **Pillar 10** | **Documentation & Packaging** | `README.md` | Doc-tests for the full token lifecycle. | `engineering-technical-writer` | `security-audit` |

### 4.2 Implementation Phase Breakdown

#### Phase 1: Models & Params (PR 1–2)
- [ ] **Task 1.1 (Types)**: `Token`, `TokenStatus`, `TokenId` prefix validation.  
  *Agent*: `engineering-software-architect` | *Skill*: `security-audit`
- [ ] **Task 1.2 (Params)**: `CreateTokenParams` with optional `save_only`.  
  *Agent*: `engineering-backend-architect` | *Skill*: `security-audit`
- [ ] **Task 1.3 (Unit Tests)**: masked-number regex; no PAN present.  
  *Agent*: `agency-api-tester` | *Skill*: `security-audit`

#### Phase 2: Endpoints (PR 3–6)
- [ ] **Task 2.1 (Create)**: `POST /v1/tokens` with `pk_`.  
  *Agent*: `engineering-backend-architect` | *Skill*: `security-audit`
- [ ] **Task 2.2 (Retrieve)**: `GET /v1/tokens/:id` with `sk_`.  
  *Agent*: `engineering-backend-architect` | *Skill*: `security-audit`
- [ ] **Task 2.3 (Errors)**: invalid id, 404.  
  *Agent*: `security-application-security-engineer` | *Skill*: `security-audit`

#### Phase 3: Delete & 204 (PR 7–8)
- [ ] **Task 3.1 (204 Handler)**: `delete_no_content` returning `Ok(())`.  
  *Agent*: `agency-api-tester` | *Skill*: `security-audit`
- [ ] **Task 3.2 (Status Mapping)**: initiated/active/inactive fixtures.  
  *Agent*: `agency-api-tester` | *Skill*: `security-audit`

#### Phase 4: Verification & Sign-Off (PR 9–10)
- [ ] **Task 4.1 (Wire Tests)**: full lifecycle round-trip.  
  *Agent*: `agency-api-tester` | *Skill*: `security-audit`
- [ ] **Task 4.2 (G-Review)**: peer review on PAN handling.  
  *Agent*: `agency-code-reviewer` | *Skill*: `security-audit`
- [ ] **Task 4.3 (Documentation)**: token lifecycle doc-tests.  
  *Agent*: `engineering-technical-writer` | *Skill*: `security-audit`

---

## 5. Security Gateway (Powered by security-audit)

### 5.1 Trust Boundaries & Security Actors
- **Trust Boundaries**:
  - `Tokenization Boundary`: PAN → `token_...` substitution.
  - `Key Boundary`: `pk_` creates; `sk_` retrieves/deletes.
- **Authorized Principals**: storefront (`pk_`), merchant backend (`sk_`).

### 5.2 Threat Modeling & Abuse Matrix (STRIDE / ASVS)

| Threat Vector | Severity | Vulnerability Mechanism | Mitigating Control & Security Invariant |
| :--- | :--- | :--- | :--- |
| **PAN Retention** | Critical | Full card number stored in the token model | Only masked numbers in `Token`; no `Debug` on PAN holders. |
| **Token Theft** | High | Leaked `token_...` charged by an attacker | Tokens are merchant-scoped server-side; `sk_` required to charge. |
| **Stale Token Charge** | High | Charging an `inactive` token | `Active` status required before off-session charge. |
| **204 Misparse** | Medium | Treating empty body as JSON causes errors | Dedicated `delete_no_content` path. |
| **Enumeration** | Low | Guessing token ids | High-entropy token ids; 404 on unknown. |

### 5.3 Cryptographic & Data Protection Controls
- **Substitution**: PAN replaced by an opaque token.
- **Masking**: only last-4 retained for display.
- **Status Gate**: `Active` required pre-charge.

### 5.4 Automated Security Verification Checklist
- [ ] **PAN Test**: no 16-digit sequence in `format!("{:?}", token)`.
- [ ] **204 Test**: delete returns `Ok(())`.
- [ ] **Status Gate Test**: inactive token blocks charge.
- [ ] **Key Test**: `delete` unreachable with `pk_`.

### 5.5 Security Gateway (G2) Sign-Off Checklist
- [ ] Tokenization verified against OWASP ASVS Level 2.
- [ ] Zero PAN retention.
- [ ] Status gating enforced.
- [ ] Independent reviewer sign-off; zero open findings.

---

## 6. Validation Gates & Acceptance Sign-Off

**Status: Ready for Implementation.**

### G0 — Build, lint & format
- [ ] **US-4.1.0-G0:** `cargo build && cargo clippy -- -D warnings && cargo fmt --check` clean.

### G1 — Test coverage
- [ ] **US-4.1.0-G1:** ≥90% line / ≥85% branch on the tokens module.

### G2 — Security audit
- [ ] **US-4.1.0-G2:** PAN-retention audit passes with zero critical/high findings.

### G3 — Architecture and maintainability
- [ ] **US-4.1.0-G3:** Token module depends only on the kernel.

### G-Review — Code review
- [ ] **US-4.1.0-G-Review:** Peer review on PAN handling and 204 path.

### G-Borrow — Rust zero-clone validation
- [ ] **US-4.1.0-G-Borrow:** `TokenId` avoids needless `String` cloning.

### G-Complexity — Cyclomatic complexity
- [ ] **US-4.1.0-G-Complexity:** $CC \le 6$; nesting $\le 2$.

### G-Docs — Public docs & examples
- [ ] **US-4.1.0-G-Docs:** 100% public symbols documented; lifecycle example runnable.

### G-Law — Law validation
- [ ] **US-4.1.0-G-Law:** PCI-DSS 4.0 Requirement 3.4 tokenization guidance certified.

### G4 — Integrated acceptance
- [ ] **US-4.1.0-G4:** Create→retrieve→delete round-trip verified against the mock server.
