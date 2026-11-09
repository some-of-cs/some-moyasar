---
description: Browse, search, and enable specialist AI agents from the bundled msitarzewski/agency-agents catalog (all 18 divisions and 283 agents) or target an industry domain with --field.
argument-hint: [--field <software|product|marketing|finance|research|healthcare|game-dev|gis> | --list-fields | --list-divisions | --search <keyword> | --divisions <d1,d2> | --agents <a1,a2> | --all]
---

# `/select-agents` — Configure Your Project's Specialist Agent Roster & Field Profile

Arguments: `$ARGUMENTS`

1. Load `skills/agency-roster-selector/SKILL.md` and `profiles/fields.json`.
2. Run `${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/select-agents.sh $ARGUMENTS` to list `--field` presets (`--list-fields`), list divisions (`--list-divisions`), search the 283 bundled agents from [`msitarzewski/agency-agents`](https://github.com/msitarzewski/agency-agents), or enable the chosen field/divisions/agents into the current workspace (writing `.agents/field-profile.json` when `--field` is specified).
