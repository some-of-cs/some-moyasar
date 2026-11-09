# Reference 05: GitHub Projects Kanban & In-PR Conversation Reporting

## 1. Visual Kanban Tracking via GitHub Projects (`gh project`)

Instead of maintaining local `KANBAN.md` files that cause merge conflicts across parallel branches, all task states are tracked visually in **GitHub Projects v2** alongside a local machine-readable `.agents/checkpoints/<module-id>/checkpoint.json`.

### Standard Kanban Columns
- **`Todo`**: Planned Modules, User Stories, and Implementation Units created during `spec-decomposition`.
- **`In Progress`**: Active worktree provisioned and implementer agent spawned.
- **`In Review / PR Open`**: Pull Request opened (`gh pr create`) and Code Reviewing Council active.
- **`Merged`**: Pull Request approved by the Council and merged (`gh pr merge`).

Use `./scripts/gh-kanban-sync.sh` to synchronize states between GitHub Projects and `.agents/checkpoints/<module-id>/checkpoint.json`.

---

## 2. In-PR Conversation Reporting (Zero Local Report Files)

Instead of generating local markdown files under `.agents/reports/`, **every report is posted directly to the GitHub PR conversation** via `gh pr comment <PR> --body "..."` or `./scripts/post-pr-report.sh`.

### Required PR Conversation Comments Before Merge
1. **Implementation & Verification Report** (posted by the Unit/Patch Implementer or Story/Module Leader):
   - Scope, branch, and commit SHA
   - Complete inventory of created/modified files
   - Verbatim build, linter, formatter, and test outputs
2. **Security Audit & Cyclomatic Complexity Report** (posted by the Reviewing Council):
   - Per-function Cyclomatic Complexity table ($CC \le 6$ target, $\le 10$ cap, $>15$ blocker)
   - Security checklist results (`security-audit` skill)
3. **Gate G-Review Formal Sign-Off** (posted by the Council President):
   - Structured `### Gate G-Review Approval` verdict unlocking `gh pr merge`.
