# Reference 02: Turning Monolithic Docs into Modules, User Stories & Dynamic Implementation Units

## 1. The 4-Tier Decomposition Pipeline

```text
Monolithic Specification / Research Doc / PRD / Model Brief
  └── Module 1..M (Bounded Contexts / Workstreams / Subsystems)
        └── User Story M.1..M.S (Cohesive Feature or Domain Deliverables)
              └── Implementation Units 1..K (Parallel, Non-Blocking Vertical Slices)
```

## 2. Dynamic Implementation Unit Sizing Matrix ($K$ Is Not Fixed at 5)

Implementation units per User Story **do not have to be 5**—a story can have fewer (e.g., 2–3) or more (e.g., 6–8+) depending on its actual architectural or deliverable footprint.

Compute the **Story Decomposition Score ($S_{\text{story}}$)** across 4 dimensions using the active `--field` preset (`profiles/fields.json`):

| Dimension | Weight | Score `1` (Low) | Score `2` (Medium) | Score `3` (High) | Score `4` (Very High) |
| :--- | :---: | :--- | :--- | :--- | :--- |
| **1. Functional / Deliverable Surface ($D_1$)** | $35\%$ | 1–2 items (endpoints/components or sections/channels/models/hypotheses) | 3–4 items | 5–6 items | 7+ items |
| **2. Artifact Volume ($D_2$)** | $25\%$ | $< 300$ LOC *(or $< 1,000$ words / 1–2 sheets)* | $300 - 700$ LOC *(or $1,000 - 2,500$ words / 3–4 sheets)* | $700 - 1,400$ LOC *(or $2,500 - 5,000$ words / 5–6 sheets)* | $> 1,400$ LOC *(or $> 5,000$ words / 7+ sheets)* |
| **3. Logic / Analytical Complexity ($D_3$)** | $20\%$ | Simple CRUD / UI layout / narrative summary | State transitions / validation / structured synthesis | Crypto / streaming / multi-scenario sensitivity | Concurrency / distributed consensus / multi-variable statistical or regulatory modeling |
| **4. Orthogonal File Boundaries ($D_4$)** | $20\%$ | 1–2 files/modules/chapters | 3–4 independent files/modules/chapters | 5–6 independent files/modules/chapters | 7+ independent files/modules/chapters |

$$S_{\text{story}} = 0.35 D_1 + 0.25 D_2 + 0.20 D_3 + 0.20 D_4$$

### Recommended Implementation Unit Count ($K$)

| Story Score ($S_{\text{story}}$) | Recommended Units ($K$) | Typical Profile |
| :---: | :---: | :--- |
| **$1.00 - 1.25$** | **1–2 Units** | Small feature, config module, or compact 2-section deliverable |
| **$1.26 - 2.00$** | **3 Units** | Focused domain entity, service client, and handler/controller or 3-part study |
| **$2.01 - 2.55$** | **4 Units** | Multi-endpoint resource, full-stack slice, or 4-channel campaign |
| **$2.56 - 3.10$** | **5 Units** | Standard complex user story (domain + operations + pagination/scenarios + edge cases) |
| **$3.11 - 3.55$** | **6–7 Units** | Large multi-component subsystem or multi-schedule model |
| **$3.56 - 4.00$** | **8+ Units (or Split Story)** | Very large story; either split into 8 orthogonal units or refactor into 2 User Stories |

## 3. Pure Deliverable Production Only & Non-Blocking Unit Invariant

1. **Pure Deliverable Production Only**:
   - Each unit $u_i \in \{1..K\}$ MUST produce real primary artifacts (source code + unit tests in software; drafted sections, models, or campaign assets + self-checks in non-software).
   - **Prohibited**: Creating a blocking "Unit $K$: Integration Tests & Review for Units $1..K-1$" unit. Such a unit cannot run in parallel with Units $1..K-1$ and violates the non-blocking invariant.
2. **Zero Inter-Unit Blocking**:
   - Before spawning Unit Implementers $1..K$, the Story Leader defines shared types/interfaces/schemas on the story base branch (`feat/us-M.S.0-<slug>`) or partitions units into disjoint files/modules/chapters so all $K$ units build, verify, and run concurrently in parallel without waiting on one another.
