#!/usr/bin/env bash
# ==============================================================================
# some-divide-and-conquer — Gate 0 Preflight Check (git & gh CLI)
# Verifies that both `git` and `gh` exist and are authenticated before starting.
# ==============================================================================
set -euo pipefail

QUIET=false
SKIP_AUTH=false
HOOK_JSON=false

for arg in "$@"; do
  case "$arg" in
    --quiet|-q) QUIET=true ;;
    --skip-auth) SKIP_AUTH=true ;;
    --hook-json) QUIET=true; HOOK_JSON=true ;;
    --help|-h)
      echo "Usage: $0 [--quiet] [--skip-auth] [--hook-json]"
      echo "Verifies that git and gh (GitHub CLI) are installed and ready before starting the plugin."
      exit 0
      ;;
  esac
done

log() {
  if [ "$QUIET" = false ]; then
    echo "$@"
  fi
}

fail() {
  if [ "$HOOK_JSON" = true ]; then
    printf '{"injectSteps":[{"errorMessage":{"userMessage":"[some-divide-and-conquer] Preflight failed: %s"}}]}\n' "$*"
    exit 0
  fi
  echo "❌ [some-divide-and-conquer] PREFLIGHT FAILED: $*" >&2
  exit 1
}

log "🔍 [Gate 0 Preflight] Checking required CLI tools (git, gh)..."

# 1. Verify git exists
if ! command -v git >/dev/null 2>&1; then
  fail "'git' is not installed or not on PATH. Install Git (>= 2.30) before starting some-divide-and-conquer."
fi

GIT_VERSION="$(git --version)"
log "  ✅ Found $GIT_VERSION"

# 2. Verify git worktree support
if ! git worktree list >/dev/null 2>&1 && ! git worktree --help >/dev/null 2>&1 && ! (git worktree -h 2>&1 | grep -q 'git worktree'); then
  fail "'git worktree' subcommand is unavailable. Upgrade Git to >= 2.30."
fi

# 3. Verify gh (GitHub CLI) exists
if ! command -v gh >/dev/null 2>&1; then
  fail "'gh' (GitHub CLI) is not installed or not on PATH. Install from https://cli.github.com/ before starting some-divide-and-conquer."
fi

GH_VERSION="$(gh --version | head -n 1)"
log "  ✅ Found $GH_VERSION"

# 4. Verify gh authentication (unless --skip-auth is passed for offline dry-runs)
if [ "$SKIP_AUTH" = false ]; then
  if ! gh auth status >/dev/null 2>&1; then
    fail "'gh' is installed but not authenticated. Run 'gh auth login -s repo,project,read:project' before starting the plugin."
  fi
  log "  ✅ GitHub CLI (gh) is authenticated"
fi

log "✅ [Gate 0 Preflight] All required tools (git, gh) verified. Ready to start some-divide-and-conquer."
if [ "$HOOK_JSON" = true ]; then
  printf '{}\n'
fi
