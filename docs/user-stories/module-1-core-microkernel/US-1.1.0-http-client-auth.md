# US-1.1.0: Thread-Safe MoyasarClient Concurrency & Key-Type Isolation

> **Spec Kit Specification**: [Specification Lifecycle Active](../../../docs/architecture/DECOMPOSITION_ROADMAP.md)  
> **Behavior Blueprint**: `features/core/modules/microkernel/behaviours/client-concurrency/`  
> **Slicing Standard**: [10-PR Vertical Slicing Framework](../README.md#2-the-10-pr-vertical-slicing-framework-rust-sdk-adaptation)  
> **Security Gateway**: `security-audit` Level 2 Enforced  
> **Quality Gates**: `(G0–G4 + G-Review + G-Borrow + G-Law)`  

---

## 1. Spec Kit Specification

### 1.1 User Story Narrative
**As a** high-concurrency Rust backend engineer building Saudi/GCC fintech microservices against the Moyasar Payment Gateway,  
**I want to** initialize an immutable, thread-safe `MoyasarClient` backed by `Arc<ClientInner>` with compile-time isolation between `SecretKey` (`sk_...`) and `PublishableKey` (`pk_...`), transmitting HTTP Basic Auth as `Authorization: Basic base64("{api_key}:")`,  
**So that** my application can execute thousands of concurrent payment operations across Tokio tasks without mutex contention, credential exposure in logs, or accidentally leaking a publishable key into a backend-only route.

#### 1.1.1 Preconditions
- Application runtime on Rust 2024 edition with an async executor (Tokio / Actix-web / Axum).
- A valid Moyasar key: `sk_live_...`, `sk_test_...`, `pk_live_...`, or `pk_test_...`.
- The core microkernel compiles with `default-features = false, features = ["core"]`.

#### 1.1.2 Postconditions
- `MoyasarClient` clones via atomic refcount increment with zero deep heap duplication.
- Requests carry `Authorization: Basic base64("{key}:")` — key as username, empty-string password, per [Moyasar Authentication](https://docs.moyasar.com/api/authentication).
- Key material is `secrecy::SecretString` + `ZeroizeOnDrop`, and redacted from every `Debug` impl.
- A `PublishableKey` can never satisfy a `SecretKey` bound — enforced at compile time.

### 1.2 Acceptance Criteria (Gherkin Scenarios)

```gherkin
Feature: MoyasarClient Thread Safety and Key-Type Isolation

  Scenario: [AC-US-1.1.0-01] Cheap lock-free client cloning across concurrent Tokio tasks
    Given a MoyasarClient initialized with a valid SecretKey
    When 10,000 asynchronous Tokio tasks clone the MoyasarClient concurrently
    Then each task executes without lock contention or thread synchronization bottlenecks
    And the memory footprint of the client remains constant across all clones.

  Scenario: [AC-US-1.1.0-02] HTTP Basic Auth encoding with empty password
    Given a MoyasarClient configured with SecretKey "sk_test_YOUR_SECRET_KEY"
    When a request is dispatched to the Moyasar API
    Then the outgoing Authorization header equals "Basic " + base64("sk_test_YOUR_SECRET_KEY:")
    And the decoded credential is "sk_test_YOUR_SECRET_KEY:" with a trailing colon and empty password.

  Scenario: [AC-US-1.1.0-03] Compile-time rejection of publishable keys on secret-only routes
    Given a MoyasarClient initialized with a PublishableKey "pk_test_abc123"
    When code attempts to compile a call to client.payments().retrieve(id)
    Then the Rust compiler rejects the program with a type mismatch
    And no runtime 403 Forbidden response is ever generated for this mistake.

  Scenario: [AC-US-1.1.0-04] Key prefix validation and live/test mode detection
    Given a caller providing "sk_live_abc", "sk_test_abc", "pk_live_abc", or "pk_test_abc"
    When SecretKey::new() or PublishableKey::new() parses it
    Then valid prefixes are accepted and the live/test mode is recorded
    And an invalid prefix such as "abc123" returns Err(MoyasarError::InvalidKey).

  Scenario: [AC-US-1.1.0-05] Secret zeroization and debug log redaction
    Given a MoyasarClient configured with live API secret "sk_live_987654321secret"
    When the client is formatted using the "{:?}" Debug trait specifier
    Then the output prints "[REDACTED]" for the secret key
    And upon dropping the client, secret memory bytes are zeroized using volatile writes.

  Scenario: [AC-US-1.1.0-LAW] SAMA credential protection and PCI-DSS 4.0 compliance
    Given any client instantiation or network transmission
    When API credentials or authentication tokens are loaded or transmitted
    Then the system enforces TLS encryption in transit via rustls, strict memory zeroization on drop,
         and complete absence of plaintext credentials in logs, fulfilling PCI-DSS 4.0 Requirements 3.4 and 8.3.
```

### 1.3 Test Cases & Traceability Matrix

| Test Case ID | Target Criterion | Test Type | Execution Level | Verification Assertion & Pass Criteria |
| :--- | :--- | :--- | :--- | :--- |
| `TC-1.1.0-01` | `AC-US-1.1.0-01` | Concurrency | Integration / Stress | 10k Tokio spawns complete; `Arc::strong_count` integrity; zero data races under `loom`. |
| `TC-1.1.0-02` | `AC-US-1.1.0-02` | Functional | Unit / Contract | `base64("{key}:")` produces a trailing colon; header matches golden byte vector. |
| `TC-1.1.0-03` | `AC-US-1.1.0-03` | Type Safety | Compile-fail (`trybuild`) | `pk_` client calling `.retrieve()` fails to compile with expected error text. |
| `TC-1.1.0-04` | `AC-US-1.1.0-04` | Validation | Unit | Table-driven: 4 valid prefixes accepted, 5 invalid rejected with `InvalidKey`. |
| `TC-1.1.0-05` | `AC-US-1.1.0-05` | Security | Unit / SAST | `format!("{:?}", client)` contains neither `"sk_live"` nor `"sk_test"`. |
| `TC-1.1.0-LAW` | `AC-US-1.1.0-LAW` | Compliance | Static / Audit | TLS-only transport verified; zero secret keys in memory dumps and trace spans. |

---

## 2. Behavior-Centric Architecture & Folder Blueprint

### 2.1 Colocated Behaviour Directory Tree

```
features/core/modules/microkernel/behaviours/client-concurrency/
├── services/        # MoyasarClient core struct, ClientInner, HTTP execution engine
├── logic/           # Basic Auth encoder, key zeroizer, header injection
├── types/           # MoyasarClientConfig, SecretKey, PublishableKey, KeyMode
└── constants/       # DEFAULT_API_BASE, DEFAULT_TIMEOUT_SECS
```

### 2.2 Scoping & Promotion Blueprint
- **Tier 1 (Behaviour-Local)**: `ClientInner` atomic state and volatile zeroization helpers.
- **Tier 2 (Module-Shared)**: `MoyasarClientConfig` shared across all microkernel services.
- **Tier 3 (Feature-Shared)**: `MoyasarClient` re-exported for all domain plugins.
- **Tier 4 (Global-Shared)**: Re-exported from the `some-moyasar` facade via `prelude::*`.

---

## 3. Technical Implementation Plan

### 3.1 Domain Invariants & Storage Schema
- **Rust Memory Law**: Strict borrowing (`&self`); cloning `MoyasarClient` only bumps the `Arc` counter.
- **Key-Type Isolation Law**: `SecretKey` and `PublishableKey` are non-convertible newtypes with no `From`/`Into` between them.
- **Auth Encoding Law**: `base64("{api_key}:")` — the trailing colon is mandatory even with an empty password.
- **Redaction Law**: `#[derive(Debug)]` is forbidden on any struct holding key material.

```rust
pub struct SecretKey(SecretString);
pub struct PublishableKey(SecretString);

impl SecretKey {
    pub fn new(raw: &str) -> Result<Self, MoyasarError> {
        if raw.starts_with("sk_live_") || raw.starts_with("sk_test_") {
            Ok(Self(SecretString::from(raw.to_owned())))
        } else {
            Err(MoyasarError::InvalidKey)
        }
    }
}

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretKey([REDACTED])")
    }
}
```

### 3.2 Activity & Interaction Diagram

```mermaid
sequenceDiagram
    autonumber
    actor Service as Microservice Handler
    participant Client as MoyasarClient (Arc<ClientInner>)
    participant Auth as SecretKey / BasicEncoder
    participant Builder as reqwest::RequestBuilder
    participant Transport as reqwest + rustls
    participant API as api.moyasar.com

    Service->>Client: client.payments().create(params)
    Client->>Auth: expose_secret() -> zeroizing borrow
    Auth->>Builder: Authorization: Basic base64("{sk_...}:")
    Client->>Builder: Content-Type: application/json
    Builder->>Transport: POST /v1/payments
    Transport->>API: TLS wire transmission
    API-->>Transport: 201 Created Payment JSON
    Transport-->>Service: Deserialized Payment
```

---

## 4. Granular Implementation Tasks

### 4.1 The 10-PR Vertical Slicing Implementation Plan

| PR # | Pillar | Architectural Layer | Primary Scope & Files | Deliverables & Acceptance | Accountable Role / Agent | Assigned Skills |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **PR 1** | **Pillar 1** | **Microkernel Domain Logic** | `src/kernel/client.rs` | `MoyasarClient`, `ClientInner`, `Send + Sync` assertions. | `engineering-software-architect` | `security-audit` |
| **PR 2** | **Pillar 2** | **Key Types & Config** | `src/kernel/auth.rs` | `SecretKey`, `PublishableKey`, prefix validation, `ZeroizeOnDrop`. | `security-identity-access-engineer` | `security-audit` |
| **PR 3** | **Pillar 3** | **Transport Wire Adapter** | `src/kernel/transport.rs` | `Authorization: Basic base64("{key}:")` injection. | `engineering-backend-architect` | `security-audit` |
| **PR 4** | **Pillar 4** | **Concurrency Engine** | `src/kernel/client.rs` | Lock-free distribution across Tokio workers; pool tuning. | `engineering-backend-architect` | `security-audit` |
| **PR 5** | **Pillar 5** | **Error Taxonomy & Redaction** | `src/kernel/auth.rs`, `error.rs` | Sanitized `Debug` on all key structs. | `security-application-security-engineer` | `security-audit` |
| **PR 6** | **Pillar 6** | **Resource Facade Mounting** | `src/lib.rs` | Feature-gated accessor mounting; `prelude` wiring. | `engineering-software-architect` | `security-audit` |
| **PR 7** | **Pillar 7** | **Zero-Copy Hardening** | `src/kernel/types.rs` | Zero-clone borrowing discipline; newtype key wrappers. | `agency-rust-refactoring-specialist` | `security-audit` |
| **PR 8** | **Pillar 8** | **Key-Type Compile Guards** | `tests/ui/publishable_rejected.rs` | `trybuild` compile-fail proof that `pk_` cannot reach secret routes. | `agency-code-reviewer` | `security-audit` |
| **PR 9** | **Pillar 9** | **Integration Wire Verification** | `tests/client_concurrency_test.rs` | 10k-task concurrency against mock TCP server; zero growth. | `agency-api-tester` | `security-audit` |
| **PR 10** | **Pillar 10** | **Documentation & Packaging** | `README.md`, `src/kernel/` | Public API docs, doc-tests, Cargo feature flags. | `engineering-technical-writer` | `security-audit` |

### 4.2 Implementation Phase Breakdown

#### Phase 1: Core Client Struct & Secret Management (PR 1–2)
- [ ] **Task 1.1 (Types)**: Define `ClientInner`, `MoyasarClientConfig`, `SecretKey`, `PublishableKey` with `Zeroize` on drop.  
  *Agent*: `engineering-software-architect` | *Skill*: `security-audit`
- [ ] **Task 1.2 (Constants)**: Pin `DEFAULT_API_BASE` (`https://api.moyasar.com/v1`) and `DEFAULT_TIMEOUT_SECS` (30).  
  *Agent*: `engineering-software-architect` | *Skill*: `security-audit`
- [ ] **Task 1.3 (Logic)**: `MoyasarClient::builder(key).build()` wrapping `Arc<ClientInner>`.  
  *Agent*: `engineering-backend-architect` | *Skill*: `security-audit`
- [ ] **Task 1.4 (Unit Tests)**: Debug redaction assertions.  
  *Agent*: `agency-api-tester` | *Skill*: `security-audit`

#### Phase 2: Auth Encoding & Transport Adapters (PR 3–6)
- [ ] **Task 2.1 (Basic Auth)**: Implement `base64("{api_key}:")` encoder; assert trailing colon.  
  *Agent*: `engineering-backend-architect` | *Skill*: `security-audit`
- [ ] **Task 2.2 (Header Injection)**: Inject `Authorization`, `Content-Type`, `Accept` on every builder.  
  *Agent*: `engineering-backend-architect` | *Skill*: `security-audit`
- [ ] **Task 2.3 (Facade Routing)**: Borrowed `&MoyasarClient` accessors with zero allocation.  
  *Agent*: `engineering-software-architect` | *Skill*: `security-audit`
- [ ] **Task 2.4 (Integration Tests)**: Mock-server header and timeout assertions.  
  *Agent*: `agency-api-tester` | *Skill*: `security-audit`

#### Phase 3: Zero-Clone Hardening & Compile Guards (PR 7–8)
- [ ] **Task 3.1 (Borrow Check)**: Audit for deep `.clone()` on client instances.  
  *Agent*: `agency-rust-refactoring-specialist` | *Skill*: `security-audit`
- [ ] **Task 3.2 (Compile Guards)**: `trybuild` UI tests for key-type rejection.  
  *Agent*: `agency-code-reviewer` | *Skill*: `security-audit`
- [ ] **Task 3.3 (Stress Testing)**: 10,000 Tokio tasks sharing one client; zero contention.  
  *Agent*: `agency-api-tester` | *Skill*: `security-audit`

#### Phase 4: Security Verification & Release Sign-Off (PR 9–10)
- [ ] **Task 4.1 (Secret Scan)**: Memory dump inspection for zero plaintext key leaks.  
  *Agent*: `security-application-security-engineer` | *Skill*: `security-audit`
- [ ] **Task 4.2 (G-Review)**: Peer review for lock-free concurrency and Rust 2024 idioms.  
  *Agent*: `agency-code-reviewer` | *Skill*: `security-audit`
- [ ] **Task 4.3 (Documentation)**: Doc-tests for `MoyasarClient` and both key types.  
  *Agent*: `engineering-technical-writer` | *Skill*: `security-audit`

---

## 5. Security Gateway (Powered by security-audit)

### 5.1 Trust Boundaries & Security Actors
- **Trust Boundaries**:
  - `Client Execution Space`: caller application memory and task pool.
  - `Transport Boundary`: TLS socket to `api.moyasar.com`.
  - `Key-Type Boundary`: compile-time split between client-side (`pk_`) and server-side (`sk_`) authority.
- **Authorized Principals**:
  - Secret Key (`sk_`): backend authority across all 37 endpoints.
  - Publishable Key (`pk_`): exactly 4 client-originated routes — `POST /v1/payments`, `POST /v1/payment_intents/:id/fulfill`, `POST /v1/tokens`, `GET /v1/applepay/initiate`.

### 5.2 Threat Modeling & Abuse Matrix (STRIDE / ASVS)

| Threat Vector | Severity | Vulnerability Mechanism | Mitigating Control & Security Invariant |
| :--- | :--- | :--- | :--- |
| **Spoofing & Key Leakage** | Critical | Secret keys logged in plaintext during debug | Hand-written `Debug` emits `[REDACTED]`. |
| **Information Disclosure** | High | Memory dump exposes discarded keys | `SecretString` + `ZeroizeOnDrop` volatile overwrite. |
| **Privilege Escalation** | Critical | Publishable key reaching a secret-only route | Rust type system; non-convertible newtypes. |
| **Tampering** | High | Plaintext HTTP downgrade | `rustls-tls` default; `http://` base rejected at build. |
| **Elevation via Debug** | Medium | Panic backtraces dumping credential structs | `#[derive(Debug)]` forbidden on key-holding structs. |

### 5.3 Cryptographic & Data Protection Controls
- **TLS Transport**: `rustls-tls` is a default feature.
- **Memory Zeroization**: `zeroize::Zeroize` on drop for all key material.
- **Debug Filter**: compiler-enforced redaction via manual `Debug` impls.
- **Basic Auth Note**: base64 is **encoding, not encryption** — confidentiality rests entirely on TLS.

### 5.4 Automated Security Verification Checklist
- [ ] **Secret Scanning**: Gitleaks and `cargo-audit` report zero hardcoded secrets.
- [ ] **Log Redaction Test**: `format!("{client:?}")` contains no API key.
- [ ] **Memory Dump Audit**: Zeroized bytes confirmed after drop.
- [ ] **Compile-Fail Guard**: `trybuild` proves `pk_` cannot reach secret routes.

### 5.5 Security Gateway (G2) Sign-Off Checklist
- [ ] Threat boundaries verified against OWASP ASVS Level 2.
- [ ] Key-type isolation verified; zero runtime 403s from mismatched keys.
- [ ] Zero plaintext credentials in telemetry or log spans.
- [ ] Independent reviewer sign-off; zero open findings.

---

## 6. Validation Gates & Acceptance Sign-Off

**Status: Ready for Implementation.**

### G0 — Build, lint & format
- [ ] **US-1.1.0-G0:** `cargo build && cargo clippy --all-targets -- -D warnings && cargo fmt --check` passes clean.

### G1 — Test coverage
- [ ] **US-1.1.0-G1:** ≥90% line and ≥85% branch coverage on the microkernel core.

### G2 — Security audit
- [ ] **US-1.1.0-G2:** Zero-leak credential audit passes with zero critical/high findings.

### G3 — Architecture and maintainability
- [ ] **US-1.1.0-G3:** Microkernel compliance; zero monolithic codegen in core.

### G-Review — Code review
- [ ] **US-1.1.0-G-Review:** Peer review by `agency-code-reviewer` across correctness, security, performance.

### G-Borrow — Rust zero-clone validation
- [ ] **US-1.1.0-G-Borrow:** Zero unauthorized deep `.clone()`; `Arc::clone` only.

### G-Complexity — Cyclomatic complexity
- [ ] **US-1.1.0-G-Complexity:** All functions $CC \le 6$; nesting $\le 2$.

### G-Docs — Public docs & examples
- [ ] **US-1.1.0-G-Docs:** 100% public symbols documented with runnable doc-tests.

### G-Law — Law validation
- [ ] **US-1.1.0-G-Law:** PCI-DSS 4.0 Requirements 3.4 & 8.3 and SAMA credential handling certified.

### G4 — Integrated acceptance
- [ ] **US-1.1.0-G4:** 10k-task concurrency verified against mock server; Basic Auth header byte-exact.
