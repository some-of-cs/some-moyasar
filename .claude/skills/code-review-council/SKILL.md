---
name: code-review-council
description: Conduct rigorous, independent multi-council reviews enforcing field-adapted quality gates (Gate G0, Gate G1, Gate G2, Gate G-Complexity, Gate G-Docs, Gate G-Review) across software, product, marketing, finance, research, healthcare, game-dev, and gis with mandatory patch-implementer remediation and in-PR conversation reporting.
---

# Federated Code & Deliverable Reviewing Council Skill

Use this skill whenever acting as a **Code Reviewer** or **Council President** at the Unit, Patch, User Story, Module, or Whole-Repository level. Automatically inspect `.agents/field-profile.json` (or `--field <FIELD_PROFILE>`, defaulting to `software`) and `profiles/fields.json` to enforce the active field's 6-gate rubric.

---

## 1. Core Principles of the Reviewing Council

1. **Zero Self-Approval**:
   - An implementer or leader agent may NEVER approve its own Pull Request. Every PR requires an independent `Code Reviewer` subagent sign-off.
2. **Massive Parallelism (Multi-Council Execution)**:
   - Multiple Reviewing Councils run in parallel—one per implementation unit, patch, story, or module—so review is never a serial bottleneck.
3. **Never Terminate Until Clean**:
   - If the Council finds any blocker, warning, security/compliance flaw, missing test/verification, or complexity violation ($CC > 10$ for code or nesting depth $> 2$ for prose/models), it posts a **`CHANGES REQUESTED`** report on the PR conversation.
   - At the Unit tier, the Unit Implementer fixes the issues and re-invokes the Council.
   - At the Story and Module tiers, the Leader spawns a dedicated **`patch-implementer`** subagent (`patch/us-M.S.0-<scope>` or `patch/module-M-<scope>`) to fix the issues via a patch PR and re-invokes the Council.
4. **100% In-Conversation PR Audit Trail**:
   - Every review iteration MUST be posted directly as a comment on the GitHub Pull Request via `gh pr comment <PR> --body "..."` (or `${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/post-pr-report.sh --field <FIELD>`). Never write local report files.

---

## 2. The 6 Mandatory Quality Gates (Field-Adapted Rubric)

### 2.1 Default Software Engineering Gates (`--field software`)

| Gate | Name | Required Skill / Tooling | Pass Threshold |
| :--- | :--- | :--- | :--- |
| **Gate G0** | **Build, Linter & Formatter** | Project compiler, linter (`clippy`, `eslint`, `ruff`, `golangci-lint`, etc.), formatter | **0 errors, 0 warnings, 100% formatted** |
| **Gate G1** | **Tests & Coverage** | Unit, contract, & integration test suite | **100% passing; $\ge 90\%$ line & $\ge 85\%$ branch coverage** |
| **Gate G2** | **Security & Crypto Audit** | `security-audit` skill | **0 Critical/High/Medium vulnerabilities; 0 leaked secrets** |
| **Gate G-Complexity** | **Cyclomatic Complexity** | `cyclomatic-complexity` skill | **Target $CC \le 6$; Ceiling $CC \le 10$; Hard Blocker $CC > 15$** |
| **Gate G-Docs** | **Documentation & Examples** | Public API docs, runnable doctests/examples, `README.md` | **100% public symbols documented & verified** |
| **Gate G-Review** | **Council President Verdict** | `code-review-council` skill | **Structured sign-off comment posted on GitHub PR** |

### 2.2 Field-Targeted Gate Criteria (`profiles/fields.json`)

| `--field` Flag | Gate G0 (Format / Build) & Gate G1 (Verification) | Gate G2 (Risk / Safety) & Gate G-Complexity (Clarity) |
| :--- | :--- | :--- |
| **`software`** *(default)* | **G0**: Compiler, linter, formatter (0 warnings). **G1**: Unit/integration tests ($\ge 90\%$ coverage). | **G2**: `security-audit` (OWASP, secrets, crypto). **G-Complexity**: Cyclomatic $CC \le 6$ (cap 10). |
| **`product`** | **G0**: Schema, template & link validation. **G1**: 100% acceptance-criteria & edge-case traceability. | **G2**: Privacy, accessibility (WCAG/Section 508) & UX dark-pattern audit. **G-Complexity**: Flow branching depth $\le 2$, unambiguous scope. |
| **`marketing`** | **G0**: Brand voice, style guide & SEO/UTM lint. **G1**: Conversion tracking, A/B hypothesis & KPI completeness. | **G2**: FTC/ad-policy compliance, claim substantiation & brand safety. **G-Complexity**: Readability grade & single-CTA clarity. |
| **`finance`** | **G0**: Formula integrity, balance reconciliation (0 rounding drift). **G1**: Stress-test & sensitivity scenario verification. | **G2**: Tax/regulatory/audit compliance (SOX/IFRS/GAAP) & PII redaction. **G-Complexity**: Model modularity & transparent assumptions. |
| **`research`** | **G0**: Citation format, bibliography & figure/table numbering check. **G1**: Statistical validity, methodology & source verification. | **G2**: Plagiarism/hallucination check, ethics & bias audit. **G-Complexity**: Argument cohesion & readability (nesting $\le 2$). |
| **`healthcare`** | **G0**: Terminology standards (ICD/SNOMED/FHIR) & template check. **G1**: Clinical evidence grading & protocol verification. | **G2**: HIPAA/PHI privacy, patient-safety & regulatory compliance. **G-Complexity**: Decision-tree clarity ($\le 2$ nested conditions). |
| **`game-dev`** | **G0**: Engine build, shader compile & asset budget check. **G1**: Frame-time budget, multiplayer sync & playtest assertions. | **G2**: Anti-cheat, economy exploit & state-desync audit. **G-Complexity**: Script $CC \le 6$ & state-machine simplicity. |
| **`gis`** | **G0**: CRS/projection validation, GeoJSON/WKT schema lint. **G1**: Spatial topology, bounds & accuracy verification. | **G2**: Geoprivacy & data-licensing compliance. **G-Complexity**: Geoprocessing pipeline modularity. |

---

## 3. Required PR Conversation Comment Template

Post the following markdown report directly to the PR conversation using `gh pr comment <PR_NUMBER> --body-file <tmpfile>` or `${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/post-pr-report.sh review --pr <PR_NUMBER> [--field <FIELD>]`:

```markdown
### Gate G-Review & Gatekeeper Council Audit Report — [APPROVED / CHANGES REQUESTED]

- **PR**: #<PR_NUMBER> (`<branch-name>`)
- **Field Profile**: `<software|product|marketing|finance|research|healthcare|game-dev|gis>`
- **Tier**: [Unit / Patch / User Story / Module / Whole-Codebase]
- **Commit Reviewed**: `<commit-sha>`

#### 1. Quality Gate Matrix
| Gate | Status | Evidence / Summary |
| :--- | :---: | :--- |
| **G0 (Build/Format or Structure/Schema)** | PASS / FAIL | `<verbatim command/check & exit code>` |
| **G1 (Tests/Coverage or Evidence/Criteria)** | PASS / FAIL | `<X passed, 0 failed / 100% criteria verified>` |
| **G2 (Security or Compliance/Risk Audit)** | PASS / FAIL | `0 vulnerabilities/compliance issues, redaction verified` |
| **G-Complexity (Cyclomatic or Structural)** | PASS / FAIL | `Max CC = X (<= 10 ceiling) / Section nesting <= 2` |
| **G-Docs (Docs, Citations & Examples)** | PASS / FAIL | `All public APIs, citations, and assumptions documented` |

#### 2. Complexity Audit (`cyclomatic-complexity`)
| File | Function / Section / Module | CC / Depth | Nesting Depth | Status |
| :--- | :--- | :---: | :---: | :---: |
| `path/to/file` | `function_or_section_name` | 3 | 1 | PASS (`<= 6` / `<= 2`) |

#### 3. Security & Compliance Audit Summary (`security-audit`)
- Input validation / schema / citation integrity: Checked
- Secret handling & PII/PHI redaction: Checked
- Domain security / regulatory invariants: Checked

#### 4. Findings & Required Remediations
- None (0 blockers, 0 warnings).

#### Verdict
**GATE G-REVIEW APPROVED** — Cleared for `gh pr merge`.
```
