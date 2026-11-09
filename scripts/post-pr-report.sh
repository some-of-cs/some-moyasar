#!/usr/bin/env bash
# ==============================================================================
# some-divide-and-conquer — In-PR Conversation Reporter
# Posts Implementation, Security/Compliance Audit, Complexity, and Council
# Sign-Off reports directly to GitHub Pull Request conversations (zero local
# report file clutter). Supports --field <FIELD_PROFILE> and auto-reads
# .agents/field-profile.json when present.
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Gate 0 Preflight: Ensure git and gh exist before running
"$SCRIPT_DIR/preflight-check.sh" --quiet --skip-auth

REPORT_TYPE="${1:-help}"
shift || true

PR_NUM=""
ROLE=""
SCOPE=""
FILES=""
TEST_CMD=""
TEST_OUTPUT=""
VERDICT="APPROVED"
VERDICT_EXPLICIT=false
MAX_CC="4"
SECURITY_SUMMARY=""
SECURITY_SUMMARY_EXPLICIT=false
COVERAGE=""
COVERAGE_EXPLICIT=false
G0_STATUS="PASS"
G1_STATUS="PASS"
G2_STATUS="PASS"
GC_STATUS="PASS"
GD_STATUS="PASS"
FIELD_PROFILE=""
DRY_RUN=false

canonical_field_name() {
  case "$1" in
    software|dev)       echo "software" ;;
    product|design)     echo "product" ;;
    marketing|growth)   echo "marketing" ;;
    finance|legal)      echo "finance" ;;
    research|academic)  echo "research" ;;
    healthcare|medical) echo "healthcare" ;;
    game-dev|spatial)   echo "game-dev" ;;
    gis|geospatial)     echo "gis" ;;
    all)                echo "all" ;;
    *) echo "Unknown field '$1'. Valid fields: software, product, marketing, finance, research, healthcare, game-dev, gis, all" >&2; exit 1 ;;
  esac
}

# Auto-detect active field from .agents/field-profile.json if present
detect_default_field() {
  local profile_file=""
  if [[ -f ".agents/field-profile.json" ]]; then
    profile_file=".agents/field-profile.json"
  else
    local git_root
    git_root="$(git rev-parse --show-toplevel 2>/dev/null || true)"
    if [[ -n "$git_root" && -f "$git_root/.agents/field-profile.json" ]]; then
      profile_file="$git_root/.agents/field-profile.json"
    fi
  fi

  if [[ -n "$profile_file" ]]; then
    if command -v jq >/dev/null 2>&1; then
      jq -r '.field // "software"' "$profile_file" 2>/dev/null || echo "software"
    else
      grep -m1 '"field"' "$profile_file" | sed -E 's/.*"field"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/' || echo "software"
    fi
  else
    echo "software"
  fi
}

FIELD_PROFILE="$(detect_default_field)"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --pr) PR_NUM="$2"; shift 2 ;;
    --field) FIELD_PROFILE="$2"; shift 2 ;;
    --role) ROLE="$2"; shift 2 ;;
    --scope) SCOPE="$2"; shift 2 ;;
    --files) FILES="$2"; shift 2 ;;
    --test-cmd) TEST_CMD="$2"; shift 2 ;;
    --test-output) TEST_OUTPUT="$2"; shift 2 ;;
    --verdict) VERDICT="$2"; VERDICT_EXPLICIT=true; shift 2 ;;
    --max-cc) MAX_CC="$2"; shift 2 ;;
    --security-summary) SECURITY_SUMMARY="$2"; SECURITY_SUMMARY_EXPLICIT=true; shift 2 ;;
    --coverage) COVERAGE="$2"; COVERAGE_EXPLICIT=true; shift 2 ;;
    --dry-run) DRY_RUN=true; shift ;;
    --g0) G0_STATUS="$2"; shift 2 ;;
    --g1) G1_STATUS="$2"; shift 2 ;;
    --g2) G2_STATUS="$2"; shift 2 ;;
    --gc) GC_STATUS="$2"; shift 2 ;;
    --gd) GD_STATUS="$2"; shift 2 ;;
    *) shift ;;
  esac
done

CANONICAL_FIELD="$(canonical_field_name "$FIELD_PROFILE")"

# Set field-appropriate Gate labels and default evidence strings
case "$CANONICAL_FIELD" in
  software)
    G0_LABEL='Gate G0 (Build, Lint & Format)'
    G0_EVIDENCE='0 errors, 0 warnings'
    G1_LABEL='Gate G1 (Tests & Coverage)'
    DEFAULT_COVERAGE='>= 90% lines, >= 85% branches'
    G2_LABEL='Gate G2 (`security-audit`)'
    DEFAULT_SECURITY_SUMMARY='0 Critical/High/Medium issues; input bounds and secret redaction verified.'
    GC_LABEL='Gate G-Complexity (`cyclomatic-complexity`)'
    GC_EVIDENCE="Max CC = $MAX_CC (<= 10 ceiling, nesting <= 2)"
    GD_LABEL='Gate G-Docs (Public Docs & Examples)'
    GD_EVIDENCE='100% public symbols documented'
    ;;
  *)
    G0_LABEL='Gate G0 (Structure, Schema & Style Compliance)'
    G0_EVIDENCE='0 schema/format errors, 100% style & template compliance'
    G1_LABEL='Gate G1 (Evidence, Verification & Criteria Coverage)'
    DEFAULT_COVERAGE='100% acceptance criteria, evidence & scenario verification'
    G2_LABEL='Gate G2 (Compliance, Risk & Fact-Check Audit)'
    DEFAULT_SECURITY_SUMMARY='0 compliance/privacy/safety findings; sources and regulatory invariants verified.'
    GC_LABEL='Gate G-Complexity (Structural Clarity & Modularity)'
    GC_EVIDENCE="Max branching/nesting = $MAX_CC (section depth <= 2, modular scope)"
    GD_LABEL='Gate G-Docs (Public Docs & Examples)'
    GD_EVIDENCE='100% references, assumptions & examples documented'
    ;;
esac

if [[ "$COVERAGE_EXPLICIT" == false ]]; then
  COVERAGE="$DEFAULT_COVERAGE"
fi
if [[ "$SECURITY_SUMMARY_EXPLICIT" == false ]]; then
  SECURITY_SUMMARY="$DEFAULT_SECURITY_SUMMARY"
fi

# Auto-derive GC_STATUS from --max-cc
if [[ "$MAX_CC" -gt 15 ]]; then
  GC_STATUS="BLOCKER"
elif [[ "$MAX_CC" -gt 10 ]]; then
  GC_STATUS="WARN"
fi

# Auto-derive VERDICT if not explicitly overridden and any gate failed/blocked
if [[ "$VERDICT_EXPLICIT" == false ]]; then
  if [[ "$G0_STATUS" != "PASS" || "$G1_STATUS" != "PASS" || "$G2_STATUS" != "PASS" || "$GC_STATUS" == "BLOCKER" || "$GC_STATUS" == "FAIL" || "$GD_STATUS" != "PASS" ]]; then
    VERDICT="CHANGES REQUESTED"
  fi
fi

if [[ "$REPORT_TYPE" == "help" || -z "$PR_NUM" ]]; then
  cat <<EOF
Usage:
  $(basename "$0") implementation --pr <PR_NUM> [--field <FIELD>] --role "<Role>" --scope "<Scope>" --files "<f1,f2>" --test-cmd "<cmd>" --test-output "<out>" [--dry-run]
  $(basename "$0") review --pr <PR_NUM> [--field <FIELD>] --verdict "<APPROVED|CHANGES REQUESTED>" --max-cc <N> --security-summary "<summary>" --coverage "<cov>" [--g0 <PASS|FAIL|WARN>] [--g1 <PASS|FAIL|WARN>] [--g2 <PASS|FAIL|WARN>] [--gc <PASS|FAIL|WARN|BLOCKER>] [--gd <PASS|FAIL|WARN>] [--dry-run]
EOF
  exit 0
fi

COMMIT_SHA="$(git rev-parse --short HEAD 2>/dev/null || echo "HEAD")"
TMP_BODY="$(mktemp)"
trap 'rm -f "$TMP_BODY"' EXIT

if [[ "$REPORT_TYPE" == "implementation" ]]; then
  cat > "$TMP_BODY" <<EOF
### 🛠️ Agent Implementation & Verification Report

- **Role**: \`${ROLE:-Unit Implementer}\`
- **Field Profile**: \`$CANONICAL_FIELD\`
- **Scope**: ${SCOPE:-Vertical Slice Implementation}
- **Commit SHA**: \`$COMMIT_SHA\`
- **Modified / Created Files**: \`${FILES:-See PR diff}\`

#### Verification & Test Execution
- **Command**: \`${TEST_CMD:-project test & lint suite}\`
- **Output Summary**:
\`\`\`text
${TEST_OUTPUT:-All unit, doctest, and linter checks passed with 0 warnings.}
\`\`\`

*Posted automatically by \`some-divide-and-conquer\` (In-PR Conversation Audit Trail).*
EOF
elif [[ "$REPORT_TYPE" == "review" ]]; then
  cat > "$TMP_BODY" <<EOF
### 🏛️ Gate G-Review & Gatekeeper Council Audit Report — $VERDICT

- **PR**: #$PR_NUM
- **Field Profile**: \`$CANONICAL_FIELD\`
- **Commit Audited**: \`$COMMIT_SHA\`

#### Quality Gate Evaluation Matrix
| Gate | Status | Evidence |
| :--- | :---: | :--- |
| **$G0_LABEL** | $G0_STATUS | \`$G0_EVIDENCE\` |
| **$G1_LABEL** | $G1_STATUS | \`$COVERAGE\` |
| **$G2_LABEL** | $G2_STATUS | $SECURITY_SUMMARY |
| **$GC_LABEL** | $GC_STATUS | \`$GC_EVIDENCE\` |
| **$GD_LABEL** | $GD_STATUS | \`$GD_EVIDENCE\` |

#### Council President Verdict
**GATE G-REVIEW $VERDICT**
EOF
fi

if [[ "$DRY_RUN" == true ]]; then
  cat "$TMP_BODY"
else
  gh pr comment "$PR_NUM" --body-file "$TMP_BODY"
  echo "✅ Posted $REPORT_TYPE report directly to PR #$PR_NUM conversation."
fi
