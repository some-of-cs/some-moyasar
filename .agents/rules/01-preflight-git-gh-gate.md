# Reference 01: Mandatory `git` & `gh` Preflight Gate

Before the **`some-divide-and-conquer`** starts any decomposition, worktree creation, or subagent orchestration, it enforces **Gate 0 (Environment & Tooling Preflight)**.

## Why `git` and `gh` Are Mandatory

1. **`git` (with `git worktree` support)**:
   - Every Module Leader, Story Leader, Unit Implementer ($1..K$), and `patch-implementer` runs in parallel inside an isolated worktree under `.worktrees/<branch-sanitized-name>`.
   - Without `git`, parallel non-blocking execution without file-lock collisions is impossible.
2. **`gh` (GitHub CLI, authenticated)**:
   - All tier promotions (`Unit -> Story -> Module -> main`) and patch merges (`patch/* -> Story/Module`) execute exclusively via `gh pr create` and `gh pr merge`.
   - All execution, security, complexity, and review reports are posted directly into PR conversations via `gh pr comment`.
   - Visual Kanban state transitions (`Todo -> In Progress -> In Review / PR Open -> Merged`) are synchronized via `gh project`.

## Automated Verification

The plugin runs `./scripts/preflight-check.sh` automatically on `SessionStart` (configured in `hooks.json`) and whenever an orchestrator starts:

```bash
./scripts/preflight-check.sh
```

### Checks Performed
1. `command -v git` exists and `git worktree list` succeeds inside a valid Git repository.
2. `command -v gh` exists (`gh --version`).
3. `gh auth status` succeeds with valid repository and project scopes (`repo`, `project` / `read:project`).
4. Remote `origin` is configured and reachable.
