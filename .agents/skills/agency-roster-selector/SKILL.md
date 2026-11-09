---
name: agency-roster-selector
description: Select, filter, and enable specialist AI agents from the bundled msitarzewski/agency-agents catalog (all 18 divisions and 283 agents) using --field industry presets (software, product, marketing, finance, research, healthcare, game-dev, gis, all), divisions, or individual agent slugs.
---

# Agency Roster Selector Skill (`msitarzewski/agency-agents`)

The **`some-divide-and-conquer`** is completely **language-, framework-, and industry-agnostic**. In addition to the 7 core orchestration agents (`Module Leader`, `Story Leader`, `Unit Implementer`, `patch-implementer`, `Code Reviewer Council President`, `Git Workflow Master`, `Quota Sentinel`), users can enable **any combination of the 283 specialist agents** from [`https://github.com/msitarzewski/agency-agents`](https://github.com/msitarzewski/agency-agents) or target an entire industry domain with a single `--field <FIELD_PROFILE>` flag.

All 18 divisions, all 283 Antigravity-converted skills, and the central field registry (`profiles/fields.json`) are bundled directly in the plugin.

---

## 1. `--field` Industry Targeting Presets (`profiles/fields.json`)

| `--field` Flag | Aliases | Auto-Enabled Divisions |
| :--- | :--- | :--- |
| **`software`** *(default)* | `software`, `dev` | `engineering,security,testing` |
| **`product`** | `product`, `design` | `product,design,project-management,research` |
| **`marketing`** | `marketing`, `growth` | `marketing,paid-media,sales,design` |
| **`finance`** | `finance`, `legal` | `finance,specialized,support` |
| **`research`** | `research`, `academic` | `academic,research,specialized` |
| **`healthcare`** | `healthcare`, `medical` | `healthcare,security,academic,support` |
| **`game-dev`** | `game-dev`, `spatial` | `game-development,spatial-computing,design,testing` |
| **`gis`** | `gis`, `geospatial` | `gis,engineering,testing` |
| **`all`** | `all` | All 18 divisions (all 283 specialist agents) |

---

## 2. Available Divisions (All 18 Divisions / 283 Agents Allowed)

| Division Key | Display Name | Agent Examples |
| :--- | :--- | :--- |
| `engineering` | **Engineering** | `backend-architect`, `frontend-developer`, `ai-engineer`, `devops-automator`, `database-optimizer`, `rust-refactoring-specialist`, `software-architect`, `api-platform-engineer`, `data-engineer`, `mobile-app-builder`, `git-workflow-master`, `code-reviewer` |
| `security` | **Security** | `application-security-engineer`, `ai-generated-code-security-auditor`, `penetration-tester`, `cloud-security-architect`, `compliance-auditor`, `security-architect` |
| `testing` | **Testing** | `api-tester`, `performance-benchmarker`, `test-automation-engineer`, `reality-checker`, `accessibility-auditor`, `test-results-analyzer` |
| `product` | **Product** | `product-manager`, `dx-engineer`, `sprint-prioritizer`, `feedback-synthesizer`, `trend-researcher` |
| `design` | **Design** | `ui-designer`, `ux-researcher`, `ux-architect`, `brand-guardian`, `whimsy-injector` |
| `project-management` | **Project Management** | `project-shepherd`, `studio-operations`, `senior-project-manager`, `jira-workflow-steward` |
| `specialized` | **Specialized** | `agents-orchestrator`, `mcp-builder`, `workflow-architect`, `lsp-index-engineer`, `model-qa-specialist` |
| `game-development` | **Game Development** | `unity-architect`, `unreal-systems-engineer`, `godot-gameplay-scripter`, `roblox-systems-scripter`, `blender-add-on-engineer`, `level-designer` |
| `spatial-computing` | **Spatial Computing** | `xr-interface-architect`, `visionos-spatial-engineer`, `xr-immersive-developer` |
| `finance` | **Finance** | `financial-analyst`, `fp-a-analyst`, `investment-researcher`, `bookkeeper-controller`, `tax-strategist` |
| `healthcare` | **Healthcare** | `clinical-evidence-agent`, `healthcare-innovation-strategist`, `sovereign-health-systems-agent` |
| `gis` | **GIS** | `spatial-data-scientist`, `bim-gis-specialist`, `cartography-designer`, `geoai-ml-engineer` |
| `research` | **Research** | `research-synthesist` |
| `academic` | **Academic** | `anthropologist`, `statistician`, `historian`, `psychologist` |
| `marketing` | **Marketing** | `content-creator`, `growth-hacker`, `seo-specialist`, `developer-community-builder` |
| `paid-media` | **Paid Media** | `ppc-campaign-strategist`, `ad-creative-strategist`, `programmatic-display-buyer` |
| `sales` | **Sales** | `account-strategist`, `sales-engineer`, `proposal-strategist` |
| `support` | **Support** | `support-responder`, `infrastructure-maintainer`, `analytics-reporter` |

---

## 3. How to Select & Enable Agents for Your Project

Use `${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/select-agents.sh` to browse, filter, or install field presets into your workspace's `.agents/agents/`, `.agents/skills/`, and `.agents/field-profile.json`:

```bash
# List all --field presets and their auto-enabled divisions
${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/select-agents.sh --list-fields

# Target an entire industry domain with a single --field flag (writes .agents/field-profile.json)
${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/select-agents.sh --field research --target /path/to/your/project

# List all 18 divisions and agent counts
${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/select-agents.sh --list-divisions

# Search agents by keyword (e.g., python, react, security, rust, kubernetes, game)
${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/select-agents.sh --search "typescript"

# Enable all agents in specific divisions (e.g., engineering + security + testing)
${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/select-agents.sh --divisions engineering,security,testing --target /path/to/your/project

# Enable specific individual agents by slug
${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/select-agents.sh --agents backend-architect,frontend-developer,application-security-engineer,api-tester --target /path/to/your/project

# Enable ALL 283 agents from all 18 divisions
${CLAUDE_PLUGIN_ROOT:-${ANTIGRAVITY_PLUGIN_ROOT:-.}}/scripts/select-agents.sh --all --target /path/to/your/project
```

---

## 4. Mapping Selected Agents to the Divide-and-Conquer Tiers

When the user selects their project roster (via `--field`, `--divisions`, or `--agents`), the plugin automatically wires them into two roles:
1. **Unit & Patch Implementer Specialists**: Domain/language agents (e.g., `frontend-developer`, `backend-architect`, `financial-analyst`, `research-synthesist`, `content-creator`, `unity-architect`) execute the parallel Implementation Units ($1..K$) and `patch-implementer` tasks.
2. **Reviewing Council Specialists**: Quality & governance agents (e.g., `code-reviewer`, `application-security-engineer`, `compliance-auditor`, `statistician`, `brand-guardian`, `reality-checker`) form the parallel **Reviewing Councils** enforcing the active field's 6 quality gates (`profiles/fields.json`).
