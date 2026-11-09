---
description: Run an independent Reviewing Council audit on a Pull Request enforcing the active field's 6 quality gates (Gates G0, G1, G2, G-Complexity, G-Docs, G-Review) and post the verdict directly to the GitHub PR conversation.
argument-hint: <pr-number> [--field <software|product|marketing|finance|research|healthcare|game-dev|gis>]
---

# `/council-review` — Independent Reviewing Council Audit

Target Pull Request (and optional `--field` preset): `$ARGUMENTS`

1. Load `skills/code-review-council/SKILL.md`, `skills/security-audit/SKILL.md` (`cloudflare/security-audit-skill`), `skills/cyclomatic-complexity/SKILL.md`, and `profiles/fields.json` (or `.agents/field-profile.json`).
2. Inspect the PR diff (`gh pr diff <pr-number>`) and run the project's build/format/schema, linter, and verification/test suites for the active `--field` profile (default: `software`).
3. Evaluate all 6 quality gates for the active `--field` preset:
   - **Gate G0**: Build, linter & formatter (`software`/`game-dev`) or Structure, schema, citation & style compliance (`product`/`marketing`/`finance`/`research`/`healthcare`/`gis`).
   - **Gate G1**: Unit/integration tests $\ge 90\%$ coverage (`software`) or 100% acceptance-criteria, evidence & scenario verification (non-software).
   - **Gate G2 (`security-audit` / Compliance Audit)**: Full security audit (OWASP, secrets, crypto) or domain regulatory/privacy/fact-check audit (WCAG, FTC, SOX/GAAP, HIPAA, geoprivacy, plagiarism).
   - **Gate G-Complexity (`cyclomatic-complexity`)**: Per-function Cyclomatic Complexity $CC \le 6$ target ($CC \le 10$ ceiling, $CC > 15$ blocker) for code, or Structural Complexity Mode (section nesting $\le 2$, single responsibility, zero circular references) for prose/models.
   - **Gate G-Docs**: Public API docstrings, citations, assumptions, examples, and README.
4. Post the complete audit report and `GATE G-REVIEW APPROVED` or `CHANGES REQUESTED` verdict directly to the GitHub PR conversation using `${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/post-pr-report.sh review --pr <pr-number> [--field <FIELD>] ...`.
5. If changes are requested on a Story or Module PR, instruct the Leader to spawn a dedicated **`patch-implementer`** subagent (`patch/*` branch & PR).
