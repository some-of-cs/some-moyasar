#!/usr/bin/env bash
# ==============================================================================
# some-divide-and-conquer — Universal Tri-Platform Plugin Installer
# Supports Google Antigravity, Claude Code, and OpenAI Codex CLI.
# Enforces Gate 0 (git + gh verification) before installing the plugin, skills,
# slash commands, core orchestrators, field profiles, and msitarzewski/agency-agents.
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLUGIN_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FIELDS_JSON="$PLUGIN_ROOT/profiles/fields.json"

TARGET_DIR="$(pwd)"
TOOL_PLATFORM="all"
FIELD_PROFILE="software"
DIVISIONS_CSV="engineering,security,testing"
DIVISIONS_EXPLICIT=false
AGENTS_CSV=""
INSTALL_ALL_AGENTS=false
SKIP_AUTH=false

resolve_field_divisions() {
  case "$1" in
    software|dev)       echo "engineering,security,testing" ;;
    product|design)     echo "product,design,project-management,research" ;;
    marketing|growth)   echo "marketing,paid-media,sales,design" ;;
    finance|legal)      echo "finance,specialized,support" ;;
    research|academic)  echo "academic,research,specialized" ;;
    healthcare|medical) echo "healthcare,security,academic,support" ;;
    game-dev|spatial)   echo "game-development,spatial-computing,design,testing" ;;
    gis|geospatial)     echo "gis,engineering,testing" ;;
    all)                echo "all" ;;
    *) echo "Unknown field '$1'. Valid fields: software, product, marketing, finance, research, healthcare, game-dev, gis, all" >&2; exit 1 ;;
  esac
}

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
    *) echo "software" ;;
  esac
}

usage() {
  cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Install the some-divide-and-conquer into a target project workspace for:
  • Google Antigravity (.agents/, AGENTS.md)
  • Claude Code        (.claude/, CLAUDE.md, commands/, agents/, skills/)
  • OpenAI Codex CLI   (.codex/, AGENTS.md, skills/)

Verifies that both 'git' and 'gh' are installed before starting installation.

Options:
  --target <DIR>              Target project workspace directory (default: current directory)
  --tool <PLATFORM>           Target platform: antigravity | claude | codex | all (default: all)
  --field <FIELD>             Industry field preset: software | product | marketing | finance |
                              research | healthcare | game-dev | gis | all (default: software)
  --divisions <d1,d2,...>     Explicit Agency-Agents divisions override (default: from --field)
  --agents <a1,a2,...>        Specific Agency-Agents slugs to enable
  --all-agents                Enable all 283 agents across all 18 divisions
  --skip-auth                 Skip 'gh auth status' check during offline CI/testing
  -h, --help                  Show this help message
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) TARGET_DIR="$2"; shift 2 ;;
    --tool) TOOL_PLATFORM="$2"; shift 2 ;;
    --field) FIELD_PROFILE="$2"; shift 2 ;;
    --divisions) DIVISIONS_CSV="$2"; DIVISIONS_EXPLICIT=true; shift 2 ;;
    --agents) AGENTS_CSV="$2"; shift 2 ;;
    --all-agents) INSTALL_ALL_AGENTS=true; shift ;;
    --skip-auth) SKIP_AUTH=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; usage; exit 1 ;;
  esac
done

# Validate field preset and resolve divisions if --divisions was not explicitly overridden
RESOLVED_FIELD_DIVISIONS="$(resolve_field_divisions "$FIELD_PROFILE")"
CANONICAL_FIELD="$(canonical_field_name "$FIELD_PROFILE")"
if [[ "$DIVISIONS_EXPLICIT" == false ]]; then
  DIVISIONS_CSV="$RESOLVED_FIELD_DIVISIONS"
fi

echo "======================================================================"
echo "  Installing some-divide-and-conquer (Platform: $TOOL_PLATFORM, Field: $CANONICAL_FIELD)"
echo "======================================================================"

# 1. Mandatory Gate 0 check: Ensure git and gh exist before starting!
if [[ "$SKIP_AUTH" == true ]]; then
  "$SCRIPT_DIR/preflight-check.sh" --skip-auth
else
  "$SCRIPT_DIR/preflight-check.sh"
fi

# 2. Create base directories & install Antigravity / Universal layout (.agents/)
mkdir -p "$TARGET_DIR/.agents/rules" \
         "$TARGET_DIR/.agents/skills" \
         "$TARGET_DIR/.agents/agents" \
         "$TARGET_DIR/.agents/assets" \
         "$TARGET_DIR/.agents/checkpoints" \
         "$TARGET_DIR/.agents/profiles" \
         "$TARGET_DIR/assets" \
         "$TARGET_DIR/profiles" \
         "$TARGET_DIR/scripts" \
         "$TARGET_DIR/.worktrees"

cp "$PLUGIN_ROOT/rules/AGENTS.md" "$TARGET_DIR/AGENTS.md"
cp -r "$PLUGIN_ROOT/rules/references/"* "$TARGET_DIR/.agents/rules/"
cp -r "$PLUGIN_ROOT/skills/"* "$TARGET_DIR/.agents/skills/"
cp "$PLUGIN_ROOT/agents/"*.md "$TARGET_DIR/.agents/agents/"
if [[ -d "$PLUGIN_ROOT/assets" ]]; then
  cp -r "$PLUGIN_ROOT/assets/"* "$TARGET_DIR/.agents/assets/"
  cp -r "$PLUGIN_ROOT/assets/"* "$TARGET_DIR/assets/"
fi
cp "$PLUGIN_ROOT/scripts/"*.sh "$TARGET_DIR/scripts/"
cp "$PLUGIN_ROOT/hooks.json" "$TARGET_DIR/hooks.json"
if [[ -f "$FIELDS_JSON" ]]; then
  cp "$FIELDS_JSON" "$TARGET_DIR/.agents/profiles/fields.json"
  cp "$FIELDS_JSON" "$TARGET_DIR/profiles/fields.json"
fi
mkdir -p "$TARGET_DIR/.claude" && cp "$PLUGIN_ROOT/.claude-plugin/hooks.json" "$TARGET_DIR/.claude/hooks.json" 2>/dev/null || true
chmod +x "$TARGET_DIR/scripts/"*.sh

# Write active field profile into $TARGET_DIR/.agents/field-profile.json
if command -v jq >/dev/null 2>&1 && [[ -f "$FIELDS_JSON" ]]; then
  jq --arg f "$CANONICAL_FIELD" --arg requested "$FIELD_PROFILE" --arg divs "$DIVISIONS_CSV" \
    '.fields[$f] + {requested_field: $requested, active_divisions_csv: $divs, registry_version: .schema_version}' \
    "$FIELDS_JSON" > "$TARGET_DIR/.agents/field-profile.json"
else
  cat > "$TARGET_DIR/.agents/field-profile.json" <<EOF
{
  "field": "$CANONICAL_FIELD",
  "requested_field": "$FIELD_PROFILE",
  "divisions_csv": "$DIVISIONS_CSV",
  "registry_version": "1.1.0"
}
EOF
fi

# 3. Enable selected specialist agents from msitarzewski/agency-agents into .agents/
if [[ "$INSTALL_ALL_AGENTS" == true ]]; then
  "$SCRIPT_DIR/select-agents.sh" --all --target "$TARGET_DIR"
elif [[ -n "$AGENTS_CSV" ]]; then
  "$SCRIPT_DIR/select-agents.sh" --agents "$AGENTS_CSV" --target "$TARGET_DIR"
elif [[ "$DIVISIONS_EXPLICIT" == true ]]; then
  "$SCRIPT_DIR/select-agents.sh" --divisions "$DIVISIONS_CSV" --target "$TARGET_DIR"
else
  "$SCRIPT_DIR/select-agents.sh" --field "$FIELD_PROFILE" --target "$TARGET_DIR"
fi

# 4. Claude Code Platform Setup (.claude/ and CLAUDE.md)
if [[ "$TOOL_PLATFORM" == "claude" || "$TOOL_PLATFORM" == "all" ]]; then
  mkdir -p "$TARGET_DIR/.claude/skills" \
           "$TARGET_DIR/.claude/agents" \
           "$TARGET_DIR/.claude/commands" \
           "$TARGET_DIR/.claude/assets"
  cp "$PLUGIN_ROOT/rules/AGENTS.md" "$TARGET_DIR/CLAUDE.md"
  cp "$PLUGIN_ROOT/.claude-plugin/hooks.json" "$TARGET_DIR/.claude/hooks.json"
  cp -r "$TARGET_DIR/.agents/skills/"* "$TARGET_DIR/.claude/skills/"
  cp -r "$TARGET_DIR/.agents/agents/"* "$TARGET_DIR/.claude/agents/"
  cp -r "$PLUGIN_ROOT/.claude-plugin/commands/"* "$TARGET_DIR/.claude/commands/"
  if [[ -d "$PLUGIN_ROOT/assets" ]]; then
    cp -r "$PLUGIN_ROOT/assets/"* "$TARGET_DIR/.claude/assets/"
  fi
  echo "  ✅ Configured Claude Code (.claude/skills, .claude/agents, .claude/commands, .claude/assets, CLAUDE.md)"
fi

# 5. OpenAI Codex CLI Platform Setup (.codex/ and AGENTS.md)
if [[ "$TOOL_PLATFORM" == "codex" || "$TOOL_PLATFORM" == "all" ]]; then
  mkdir -p "$TARGET_DIR/.codex/skills" \
           "$TARGET_DIR/.codex/agents" \
           "$TARGET_DIR/.codex/assets"
  cp -r "$TARGET_DIR/.agents/skills/"* "$TARGET_DIR/.codex/skills/"
  cp -r "$TARGET_DIR/.agents/agents/"* "$TARGET_DIR/.codex/agents/"
  if [[ -d "$PLUGIN_ROOT/assets" ]]; then
    cp -r "$PLUGIN_ROOT/assets/"* "$TARGET_DIR/.codex/assets/"
  fi
  echo "  ✅ Configured OpenAI Codex CLI (.codex/skills, .codex/agents, .codex/assets, AGENTS.md)"
fi

echo ""
echo "✅ some-divide-and-conquer installed into: $TARGET_DIR"
echo "   • Active Field Profile:      $CANONICAL_FIELD ($TARGET_DIR/.agents/field-profile.json)"
echo "   • Antigravity & Codex Rules: $TARGET_DIR/AGENTS.md"
echo "   • Claude Code Rules:         $TARGET_DIR/CLAUDE.md"
echo "   • Reviewing Council Skills:  $TARGET_DIR/.agents/skills/ (includes cloudflare/security-audit-skill)"
echo "   • Core & Agency Roster:      $TARGET_DIR/.agents/agents/"
echo "   • Worktree Sandbox:          $TARGET_DIR/.worktrees/"
echo "======================================================================"
