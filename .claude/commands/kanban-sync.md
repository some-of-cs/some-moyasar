---
description: Synchronize Module, User Story, and Implementation Unit statuses with GitHub Projects v2 Kanban board and .agents/checkpoints/checkpoint.json.
argument-hint: <init | status> [options]
---

# `/kanban-sync` — GitHub Projects Kanban & Checkpoint Synchronization

Arguments: `$ARGUMENTS`

1. Load `skills/github-projects-kanban/SKILL.md`.
2. Execute `${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/gh-kanban-sync.sh $ARGUMENTS` to update the visual **GitHub Projects** Kanban board (`Todo`, `In Progress`, `In Review / PR Open`, `Merged`) and `.agents/checkpoints/<module-id>/checkpoint.json`.
