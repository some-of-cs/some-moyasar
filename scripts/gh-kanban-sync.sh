#!/usr/bin/env bash
# ==============================================================================
# some-divide-and-conquer — GitHub Projects v2 Kanban & Checkpoint Sync
# Replaces local KANBAN.md files with GitHub Projects + .agents/checkpoints/
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Gate 0 Preflight: Ensure git and gh exist before running
"$SCRIPT_DIR/preflight-check.sh" --quiet --skip-auth

CMD="${1:-help}"
shift || true

MODULE_ID="module-1"
ITEM_ID=""
STATUS="Todo"
BRANCH=""
WORKTREE=""
PR_NUM=""
SHA=""
TITLE=""
PROJECT_NUMBER="${GH_PROJECT_NUMBER:-}"
OWNER_OVERRIDE="${GH_PROJECT_OWNER:-}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --module) MODULE_ID="$2"; shift 2 ;;
    --id) ITEM_ID="$2"; shift 2 ;;
    --status) STATUS="$2"; shift 2 ;;
    --branch) BRANCH="$2"; shift 2 ;;
    --worktree) WORKTREE="$2"; shift 2 ;;
    --pr) PR_NUM="$2"; shift 2 ;;
    --sha) SHA="$2"; shift 2 ;;
    --title) TITLE="$2"; shift 2 ;;
    --project-number) PROJECT_NUMBER="$2"; shift 2 ;;
    --owner) OWNER_OVERRIDE="$2"; shift 2 ;;
    *) shift ;;
  esac
done

CHECKPOINT_DIR=".agents/checkpoints/$MODULE_ID"
CHECKPOINT_FILE="$CHECKPOINT_DIR/checkpoint.json"
mkdir -p "$CHECKPOINT_DIR"

NOW="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

case "$CMD" in
  init)
    cat > "$CHECKPOINT_FILE" <<EOF
{
  "moduleId": "$MODULE_ID",
  "title": "${TITLE:-$MODULE_ID}",
  "githubProjectNumber": ${PROJECT_NUMBER:-null},
  "updatedAt": "$NOW",
  "items": {}
}
EOF
    echo "✅ Initialized checkpoint store at $CHECKPOINT_FILE"
    if [[ -n "$PROJECT_NUMBER" ]]; then
      echo "📋 Linked to GitHub Project #$PROJECT_NUMBER"
    fi
    ;;

  status)
    if [[ -z "$ITEM_ID" ]]; then
      echo "ERROR: --id <unit-or-story-id> is required for 'status'" >&2
      exit 1
    fi
    if [[ ! -f "$CHECKPOINT_FILE" ]]; then
      "$0" init --module "$MODULE_ID"
    fi

    python3 - "$CHECKPOINT_FILE" "$ITEM_ID" "$STATUS" "$BRANCH" "$WORKTREE" "$PR_NUM" "$SHA" "$NOW" <<'PY'
import json, sys
path, item_id, status, branch, worktree, pr_num, sha, now = sys.argv[1:]
with open(path, "r", encoding="utf-8") as f:
    data = json.load(f)
items = data.setdefault("items", {})
entry = items.setdefault(item_id, {})
entry["status"] = status
entry["updatedAt"] = now
if branch:
    entry["branch"] = branch
if worktree:
    entry["worktree"] = worktree
if pr_num:
    entry["prNumber"] = int(pr_num)
if sha:
    entry["mergedSha"] = sha
data["updatedAt"] = now
with open(path, "w", encoding="utf-8") as f:
    json.dump(data, f, indent=2)
PY
    echo "✅ Updated $CHECKPOINT_FILE: [$ITEM_ID] -> $STATUS"

    # If a PR number and GitHub Project Number are configured, sync with GitHub Projects v2
    if [[ -n "$PR_NUM" && -n "$PROJECT_NUMBER" ]]; then
      REPO_URL="$(gh repo view --json url -q .url 2>/dev/null || true)"
      if [[ -n "$REPO_URL" ]]; then
        REPO_OWNER="${OWNER_OVERRIDE:-$(gh repo view --json owner -q .owner.login 2>/dev/null || echo "@me")}"
        ITEM_JSON="$(gh project item-add "$PROJECT_NUMBER" --owner "$REPO_OWNER" --url "$REPO_URL/pull/$PR_NUM" --format json 2>/dev/null || true)"
        if [[ -n "$ITEM_JSON" ]]; then
          PROJECT_ID="$(gh project view "$PROJECT_NUMBER" --owner "$REPO_OWNER" --format json -q .id 2>/dev/null || true)"
          FIELDS_JSON="$(gh project field-list "$PROJECT_NUMBER" --owner "$REPO_OWNER" --format json 2>/dev/null || true)"
          if [[ -n "$PROJECT_ID" && -n "$FIELDS_JSON" ]]; then
            read -r PROJ_ITEM_ID FIELD_ID OPTION_ID < <(python3 - "$ITEM_JSON" "$FIELDS_JSON" "$STATUS" <<'PY'
import json, sys
item_raw, fields_raw, target_status = sys.argv[1:]
try:
    item_id = json.loads(item_raw).get("id", "")
    fields = json.loads(fields_raw).get("fields", [])
    field_id, option_id = "", ""
    for f in fields:
        if f.get("name", "").lower() == "status":
            field_id = f.get("id", "")
            for opt in f.get("options", []):
                if opt.get("name", "").lower() == target_status.lower() or target_status.lower() in opt.get("name", "").lower():
                    option_id = opt.get("id", "")
                    break
            break
    print(f"{item_id} {field_id} {option_id}")
except Exception:
    print("")
PY
            ) || true
            if [[ -n "${PROJ_ITEM_ID:-}" && -n "${FIELD_ID:-}" && -n "${OPTION_ID:-}" ]]; then
              gh project item-edit --id "$PROJ_ITEM_ID" --project-id "$PROJECT_ID" --field-id "$FIELD_ID" --single-select-option-id "$OPTION_ID" >/dev/null 2>&1 || true
            fi
          fi
        fi
        echo "📋 Synced PR #$PR_NUM to GitHub Project #$PROJECT_NUMBER ($STATUS)"
      fi
    fi
    ;;

  *)
    cat <<EOF
Usage:
  $(basename "$0") init --module <module-id> [--title <title>] [--project-number <N>] [--owner <owner>]
  $(basename "$0") status --module <module-id> --id <item-id> --status "<Todo|In Progress|In Review|Merged>" [--branch <b_name>] [--worktree <path>] [--pr <num>] [--sha <sha>] [--project-number <N>] [--owner <owner>]
EOF
    ;;
esac
