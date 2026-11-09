---
description: Launch the full Divide-and-Conquer multi-agent pipeline (Gate 0 git/gh check, field-adapted spec decomposition, parallel .worktrees/ execution, multi-council review, patch-implementer loop, and GitHub Projects Kanban sync).
argument-hint: <spec-file-or-module-id> [--field <software|product|marketing|finance|research|healthcare|game-dev|gis>]
---

# `/divide-and-conquer` — Execute the Full Divide-and-Conquer Pipeline

Target specification or module (and optional `--field` preset): `$ARGUMENTS`

Follow these steps strictly in order:

1. **Gate 0 (`preflight-git-gh` skill)**:
   - Run `${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/preflight-check.sh` to verify that both `git` and `gh` exist and `gh auth status` is authenticated. Do not proceed if either tool is missing.
2. **Field Profile & Quota Sentinel**:
   - If `--field <FIELD>` is passed in `$ARGUMENTS`, run `${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/select-agents.sh --field <FIELD> --target .` to enable that domain's specialist divisions and write `.agents/field-profile.json` (default: `software` from `profiles/fields.json`).
   - Inspect `.agents/checkpoints/` (`checkpoint.json`), `git worktree list`, and `gh pr list --state all` so zero completed or active tasks are duplicated.
   - Ensure the single-profile `quota-sentinel` agent is active.
3. **Specification Decomposition (`spec-decomposition` skill)**:
   - If `$ARGUMENTS` points to a monolithic spec/PRD/research document, decompose it into **Modules (`module-1..M`)**, **User Stories (`us-M.1.0..us-M.S.0`)**, and **dynamically sized, non-blocking Implementation Units (`unit-1..K`)** using the field-adapted **Story Decomposition Score ($S_{\text{story}}$)** matrix.
   - Ensure every unit obeys **Pure Deliverable Production Only** (zero validation-only units) and owns orthogonal files/modules/sections so all $K$ units run 100% in parallel.
4. **Parallel Execution (`divide-and-conquer-orchestrator` skill)**:
   - Spawn the **Module Leader**, which immediately spawns parallel **User Story Leaders**, which immediately spawn all **$K$ Unit Implementers** in parallel inside `.worktrees/`.
5. **Multi-Council Review & Mandatory `patch-implementer` Loop (`code-review-council` skill)**:
   - Run parallel Reviewing Councils across every Unit, Story, and Module PR enforcing the active field's 6 Quality Gates (Gates G0, G1, G2, G-Complexity, G-Docs, G-Review).
   - If any issue is flagged at the Story or Module level, spawn a dedicated **`patch-implementer`** subagent (`patch/*` branch & PR) and re-verify until 0 issues remain.
6. **In-PR Conversation Reporting & GitHub Projects Kanban (`github-projects-kanban` skill)**:
   - Post all Implementation Reports, Security/Compliance Audits, Complexity evaluations, and Gate G-Review verdicts directly to the GitHub PR conversations (`gh pr comment` / `${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/post-pr-report.sh`).
   - Synchronize visual Kanban status with **GitHub Projects** (`${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/gh-kanban-sync.sh`) and `.agents/checkpoints/<module-id>/checkpoint.json`.
