# Reference 03: Parallel `.worktrees/` Execution & Streaming Cascading Merges

## 1. Worktree Isolation Layout

Every branch is checked out into its own directory under `.worktrees/` at the repository root:

```text
<repo-root>/
├── .worktrees/
│   ├── feat-module-1-core/
│   ├── feat-us-1.1.0-transport/
│   ├── feat-us-1.1.0-unit-1-http-client/
│   ├── feat-us-1.1.0-unit-2-retry-policy/
│   ├── feat-us-1.1.0-unit-3-error-mapping/
│   └── patch-us-1.1.0-complexity-fix/
```

## 2. Parallel Execution Pipeline

1. **Story Base Preparation**:
   - `Git Workflow Master` creates `feat/us-M.S.0-<slug>` from `feat/module-M-<slug>` and provisions `.worktrees/feat-us-M.S.0-<slug>`.
2. **Simultaneous Unit Worktree Provisioning**:
   - `Git Workflow Master` creates all $K$ unit branches (`feat/us-M.S.0/unit-1-<scope>` through `unit-K-<scope>`) and their `.worktrees/`.
3. **100% Parallel Unit Implementation & Multi-Council Review**:
   - The Story Leader spawns all $K$ Unit Implementers simultaneously.
   - As each Unit Implementer finishes coding and local testing, it immediately spawns its own independent **Code Reviewing Council** in parallel.
4. **Ordered Cascading Merge Queue ($1 \to 2 \to \dots \to K$)**:
   - `Git Workflow Master` opens each unit's PR (`gh pr create`) as soon as its Unit Code Reviewer approves.
   - `Git Workflow Master` merges Unit 1 into `feat/us-M.S.0-<slug>` via `gh pr merge <PR> --merge`.
   - Unit 2 pulls/rebases the updated story branch, verifies all tests still pass, and merges via `gh pr merge`, continuing through Unit $K$.
