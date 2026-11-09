---
name: preflight-git-gh
description: Mandatory Gate 0 preflight check that verifies git and GitHub CLI (gh) are installed, configured, and authenticated before starting any Divide-and-Conquer plugin workflow.
---

# Gate 0: `git` and `gh` Preflight Verification Skill

Before running **any** step of the `some-divide-and-conquer` (decomposition, worktree creation, GitHub Projects Kanban setup, or subagent spawning), you MUST verify that both `git` and `gh` exist and are properly authenticated.

## 1. Mandatory Preflight Command

Run the bundled preflight script at the start of the session:

```bash
${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/preflight-check.sh
```

Or run the checks directly:

```bash
command -v git >/dev/null 2>&1 || { echo "ERROR: git is not installed"; exit 1; }
command -v gh  >/dev/null 2>&1 || { echo "ERROR: gh (GitHub CLI) is not installed"; exit 1; }
git rev-parse --is-inside-work-tree >/dev/null 2>&1 || { echo "ERROR: Not inside a Git repository"; exit 1; }
gh auth status >/dev/null 2>&1 || { echo "ERROR: gh is not authenticated. Run 'gh auth login'"; exit 1; }
```

## 2. Failure Handling (Hard Stop)

If **any** check fails:
1. **Do NOT create worktrees, branches, or subagents.**
2. Report the exact missing prerequisite to the user:
   - Missing `git`: Install Git ($\ge 2.30$ for `git worktree` support).
   - Missing `gh`: Install GitHub CLI from <https://cli.github.com/>.
   - Unauthenticated `gh`: Run `gh auth login -s repo,project,read:project`.
   - Missing remote `origin`: Configure `git remote add origin <repo-url>`.
