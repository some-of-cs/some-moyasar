#!/usr/bin/env bash
# ==============================================================================
# some-divide-and-conquer — Agency Roster Selector (msitarzewski/agency-agents)
# Enables users to browse, search, and install any of the 283 agents across all
# 18 divisions from https://github.com/msitarzewski/agency-agents or target an
# entire industry domain with a single --field <FIELD_PROFILE> flag.
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLUGIN_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CATALOG_DIR="$PLUGIN_ROOT/agents/agency-catalog"
RAW_DIR="$CATALOG_DIR/raw-agents"
SKILLS_DIR="$CATALOG_DIR/antigravity-skills"
FIELDS_JSON="$PLUGIN_ROOT/profiles/fields.json"

# Gate 0 check: Ensure git and gh exist before running plugin tooling
"$SCRIPT_DIR/preflight-check.sh" --quiet --skip-auth

TARGET_DIR="$(pwd)"
ACTION=""
DIVISIONS_CSV=""
AGENTS_CSV=""
SEARCH_TERM=""
FIELD_PROFILE=""

# Map --field presets to divisions in scripts/select-agents.sh
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

write_field_profile() {
  local field_arg="$1"
  local target_root="$2"
  local canon
  canon="$(canonical_field_name "$field_arg")"
  local divs
  divs="$(resolve_field_divisions "$field_arg")"

  mkdir -p "$target_root/.agents/profiles"
  if [[ -f "$FIELDS_JSON" ]]; then
    cp "$FIELDS_JSON" "$target_root/.agents/profiles/fields.json"
  fi

  if command -v jq >/dev/null 2>&1 && [[ -f "$FIELDS_JSON" ]]; then
    jq --arg f "$canon" --arg requested "$field_arg" '.fields[$f] + {requested_field: $requested, registry_version: .schema_version}' "$FIELDS_JSON" > "$target_root/.agents/field-profile.json"
  else
    cat > "$target_root/.agents/field-profile.json" <<EOF
{
  "field": "$canon",
  "requested_field": "$field_arg",
  "divisions_csv": "$divs",
  "registry_version": "1.1.0"
}
EOF
  fi
}

usage() {
  cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Select and enable specialist AI agents from https://github.com/msitarzewski/agency-agents
(All 18 divisions / 283 specialist agents bundled).

Options:
  --field <FIELD>               Enable default divisions & rubric for an industry field:
                                software | product | marketing | finance | research |
                                healthcare | game-dev | gis | all
  --list-fields                 List all supported --field targeting presets and divisions
  --list-divisions              List all 18 divisions and agent counts
  --list-agents [DIVISION]      List available agents (optionally filtered by division)
  --search <KEYWORD>            Search agents by name, division, or description
  --divisions <d1,d2,...>       Enable all agents in the comma-separated divisions
  --agents <a1,a2,...>          Enable specific agents by slug (with or without 'agency-' prefix)
  --all                         Enable all 283 agents across all 18 divisions
  --target <DIR>                Target workspace root (default: current directory)
  -h, --help                    Show this help message

Examples:
  $(basename "$0") --list-fields
  $(basename "$0") --field research --target /path/to/project
  $(basename "$0") --list-divisions
  $(basename "$0") --search "security"
  $(basename "$0") --divisions engineering,security,testing --target /path/to/project
  $(basename "$0") --agents backend-architect,frontend-developer,api-tester --target .
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --field)
      FIELD_PROFILE="$2"
      RESOLVED_DIVS="$(resolve_field_divisions "$FIELD_PROFILE")"
      if [[ "$RESOLVED_DIVS" == "all" ]]; then
        ACTION="install-all"
      else
        ACTION="install-divisions"
        DIVISIONS_CSV="$RESOLVED_DIVS"
      fi
      shift 2
      ;;
    --list-fields) ACTION="list-fields"; shift ;;
    --list-divisions) ACTION="list-divisions"; shift ;;
    --list-agents)
      ACTION="list-agents"
      if [[ $# -gt 1 && ! "$2" =~ ^-- ]]; then
        DIVISIONS_CSV="$2"; shift 2
      else
        shift
      fi
      ;;
    --search) ACTION="search"; SEARCH_TERM="$2"; shift 2 ;;
    --divisions) ACTION="install-divisions"; DIVISIONS_CSV="$2"; shift 2 ;;
    --agents) ACTION="install-agents"; AGENTS_CSV="$2"; shift 2 ;;
    --all) ACTION="install-all"; shift ;;
    --target) TARGET_DIR="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage; exit 1 ;;
  esac
done

if [[ -z "$ACTION" ]]; then
  usage
  exit 0
fi

install_single_skill() {
  local slug="$1"
  local clean_slug="${slug#agency-}"
  local skill_src="$SKILLS_DIR/agency-$clean_slug"

  if [[ ! -d "$skill_src" ]]; then
    echo "  ⚠️  Agent skill not found: $slug (expected $skill_src)" >&2
    return 1
  fi

  mkdir -p "$TARGET_DIR/.agents/skills/agency-$clean_slug"
  cp -r "$skill_src/"* "$TARGET_DIR/.agents/skills/agency-$clean_slug/"

  # Also copy raw agent markdown if found across divisions
  local raw_file
  raw_file="$(find "$RAW_DIR" -name "*${clean_slug}*.md" | head -n 1 || true)"
  if [[ -n "$raw_file" ]]; then
    mkdir -p "$TARGET_DIR/.agents/agents"
    cp "$raw_file" "$TARGET_DIR/.agents/agents/"
  fi
}

case "$ACTION" in
  list-fields)
    echo "=========================================================================================="
    echo "  Supported --field Targeting Presets (profiles/fields.json)"
    echo "=========================================================================================="
    printf "  %-14s | %-18s | %-48s\n" "FIELD FLAG" "ALIASES" "AUTO-ENABLED DIVISIONS"
    echo "------------------------------------------------------------------------------------------"
    printf "  %-14s | %-18s | %-48s\n" "software"   "software, dev"      "engineering,security,testing"
    printf "  %-14s | %-18s | %-48s\n" "product"    "product, design"    "product,design,project-management,research"
    printf "  %-14s | %-18s | %-48s\n" "marketing"  "marketing, growth"  "marketing,paid-media,sales,design"
    printf "  %-14s | %-18s | %-48s\n" "finance"    "finance, legal"     "finance,specialized,support"
    printf "  %-14s | %-18s | %-48s\n" "research"   "research, academic" "academic,research,specialized"
    printf "  %-14s | %-18s | %-48s\n" "healthcare" "healthcare, medical" "healthcare,security,academic,support"
    printf "  %-14s | %-18s | %-48s\n" "game-dev"   "game-dev, spatial"  "game-development,spatial-computing,design,testing"
    printf "  %-14s | %-18s | %-48s\n" "gis"        "gis, geospatial"    "gis,engineering,testing"
    printf "  %-14s | %-18s | %-48s\n" "all"        "all"                "All 18 divisions (283 specialist agents)"
    echo "=========================================================================================="
    ;;

  list-divisions)
    echo "======================================================================"
    echo "  Bundled Agency-Agents Divisions (msitarzewski/agency-agents)"
    echo "======================================================================"
    printf "  %-24s | %-12s\n" "DIVISION" "AGENT COUNT"
    echo "----------------------------------------------------------------------"
    total=0
    for div_path in "$RAW_DIR"/*; do
      if [[ -d "$div_path" ]]; then
        div_name="$(basename "$div_path")"
        count="$(find "$div_path" -name "*.md" | wc -l)"
        total=$((total + count))
        printf "  %-24s | %4d agents\n" "$div_name" "$count"
      fi
    done
    echo "----------------------------------------------------------------------"
    printf "  %-24s | %4d converted Antigravity skills\n" "TOTAL" "$(ls -1d "$SKILLS_DIR"/agency-* | wc -l)"
    ;;

  list-agents)
    if [[ -n "$DIVISIONS_CSV" ]]; then
      IFS=',' read -ra DIVS <<< "$DIVISIONS_CSV"
      for div in "${DIVS[@]}"; do
        echo "=== Division: $div ==="
        find "$RAW_DIR/$div" -name "*.md" -exec basename {} .md \; | sort | sed 's/^/  - /'
      done
    else
      ls -1d "$SKILLS_DIR"/agency-* | xargs -n1 basename | sed 's/^agency-//' | sort
    fi
    ;;

  search)
    echo "🔎 Searching 283 agency-agents for '$SEARCH_TERM'..."
    grep -ri -l "$SEARCH_TERM" "$SKILLS_DIR"/agency-*/SKILL.md | while read -r match; do
      skill_dir="$(dirname "$match")"
      slug="$(basename "$skill_dir" | sed 's/^agency-//')"
      desc="$(grep -m1 '^description:' "$match" | sed 's/^description:[[:space:]]*//')"
      printf "  • %-36s — %s\n" "$slug" "$desc"
    done
    ;;

  install-divisions)
    if [[ -n "$FIELD_PROFILE" ]]; then
      write_field_profile "$FIELD_PROFILE" "$TARGET_DIR"
      echo "🎯 Configured field profile '$FIELD_PROFILE' -> divisions: $DIVISIONS_CSV"
    fi
    IFS=',' read -ra DIVS <<< "$DIVISIONS_CSV"
    installed=0
    mkdir -p "$TARGET_DIR/.agents/agents" "$TARGET_DIR/.agents/skills"
    for div in "${DIVS[@]}"; do
      if [[ ! -d "$RAW_DIR/$div" ]]; then
        echo "⚠️  Unknown division: $div" >&2
        continue
      fi
      echo "📦 Enabling division '$div' into $TARGET_DIR/.agents/..."
      while IFS= read -r md_file; do
        cp "$md_file" "$TARGET_DIR/.agents/agents/"
        base="$(basename "$md_file" .md)"
        # Match corresponding antigravity skill
        stripped="${base#${div}-}"
        if [[ -d "$SKILLS_DIR/agency-$stripped" ]]; then
          cp -r "$SKILLS_DIR/agency-$stripped" "$TARGET_DIR/.agents/skills/"
        elif [[ -d "$SKILLS_DIR/agency-$base" ]]; then
          cp -r "$SKILLS_DIR/agency-$base" "$TARGET_DIR/.agents/skills/"
        fi
        installed=$((installed + 1))
      done < <(find "$RAW_DIR/$div" -name "*.md")
    done
    echo "✅ Enabled $installed agents into $TARGET_DIR/.agents/"
    ;;

  install-agents)
    IFS=',' read -ra SLUGS <<< "$AGENTS_CSV"
    installed=0
    for slug in "${SLUGS[@]}"; do
      if install_single_skill "$slug"; then
        echo "  ✅ Enabled agent: ${slug#agency-}"
        installed=$((installed + 1))
      fi
    done
    echo "✅ Enabled $installed selected agents into $TARGET_DIR/.agents/"
    ;;

  install-all)
    if [[ -n "$FIELD_PROFILE" ]]; then
      write_field_profile "$FIELD_PROFILE" "$TARGET_DIR"
      echo "🎯 Configured field profile '$FIELD_PROFILE' -> all 18 divisions"
    fi
    echo "📦 Enabling ALL 283 agents across all 18 divisions into $TARGET_DIR/.agents/..."
    mkdir -p "$TARGET_DIR/.agents/skills" "$TARGET_DIR/.agents/agents"
    cp -r "$SKILLS_DIR"/agency-* "$TARGET_DIR/.agents/skills/"
    find "$RAW_DIR" -name "*.md" -exec cp {} "$TARGET_DIR/.agents/agents/" \;
    echo "✅ All 283 Antigravity skills and raw agent definitions enabled in $TARGET_DIR/.agents/"
    ;;
esac
