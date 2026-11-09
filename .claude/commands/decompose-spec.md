---
description: Decompose a monolithic specification, PRD, RFC, financial model, campaign brief, or research document into bounded Modules, User Stories, and dynamically sized non-blocking Implementation Units (1..K) using the field-adapted Unit Sizing Decision Matrix.
argument-hint: <path-to-spec-doc> [--field <software|product|marketing|finance|research|healthcare|game-dev|gis>]
---

# `/decompose-spec` — Monolithic Spec $\to$ Modules $\to$ Stories $\to$ Dynamic $K$ Units

Specification document to decompose (and optional `--field` preset): `$ARGUMENTS`

1. Load and follow the `spec-decomposition` skill (`skills/spec-decomposition/SKILL.md`) and `profiles/fields.json` (or `.agents/field-profile.json`).
2. Verify `git` and `gh` exist via `${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/preflight-check.sh`. If `--field <FIELD>` is provided, configure the field profile via `${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/select-agents.sh --field <FIELD> --target .`.
3. Read `$ARGUMENTS` and partition it into:
   - **Level 1: Modules (`module-1..M`)** — Bounded architectural contexts or workstreams.
   - **Level 2: User Stories (`us-M.1.0..us-M.S.0`)** — Cohesive feature or domain deliverables.
   - **Level 3: Implementation Units (`unit-1..K`)** — Calculate the weighted **Story Decomposition Score ($S_{\text{story}} = 0.35 D_1 + 0.25 D_2 + 0.20 D_3 + 0.20 D_4$)** using the active field's $D_1..D_4$ metrics to determine the exact unit count $K \in [1, N]$.
4. Verify that every unit $1..K$ obeys **Pure Deliverable Production Only** (never a validation-only or review-only unit) and has disjoint file/module/section ownership so all $K$ units execute 100% in parallel without blocking one another.
5. Sync the resulting roadmap to **GitHub Projects Kanban** and `.agents/checkpoints/` using `${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/gh-kanban-sync.sh`.
