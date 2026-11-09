---
name: cyclomatic-complexity
description: Measure a function's branching complexity (CC = 1 + decision points) for executable code (--field software|game-dev|gis) and enforce Structural Complexity Mode (nesting depth <= 2, single responsibility, zero circular references, clear decision tables) for prose and analytical deliverables (--field product|marketing|finance|research|healthcare|gis).
---

# Cyclomatic & Structural Complexity Minimization Skill (Gate G-Complexity)

Every Unit Implementer, `patch-implementer`, and Reviewing Council MUST enforce **Gate G-Complexity** across all new and modified files:
- **Mode A — Cyclomatic Complexity Mode (`--field software|game-dev` or any unit containing executable code/scripts)**: Measures per-function branching complexity ($CC \le 6$ target, $CC \le 10$ ceiling, $CC > 15$ hard blocker).
- **Mode B — Structural Complexity Mode (`--field product|marketing|finance|research|healthcare|gis` for prose/analytical deliverables)**: Enforces section nesting depth $\le 2$, single responsibility per file/section, zero circular cross-references, and clear decision tables ($\le 2$ nested conditions).

---

## 1. Mode A: Executable Code — Formula $CC = 1 + \text{Decision Points}$

Start at **`1`**, then add **`+1`** for every branching construct in the target language:

| Construct Category | Examples Across Languages (Rust, TS/JS, Python, Go, Java, C#, C++) | Count |
| :--- | :--- | :---: |
| **Conditional Branches** | `if`, `else if` / `elif`, `if let`, `let ... else`, ternary `? :` | `+1` each |
| **Pattern / Switch Arms** | Each non-default `match` arm, `case` branch, or pattern guard (`if cond`) | `+1` per non-default arm |
| **Loops** | `for`, `while`, `while let`, `do ... while`, comprehension `if` filters | `+1` each |
| **Short-Circuit Boolean Ops** | `&&` (`and`), `\|\|` (`or`), nullish coalescing with fallback logic (`??`) | `+1` per operator |
| **Error / Exception Branches** | `catch` / `except` blocks, `?` / `try` early-error propagation where recovery logic branches | `+1` per handler |

*(Note: `else` and `default` / `_ =>` catch-all arms do **not** add `+1` because they share the branch opened by `if` or `switch`/`match`.)*

---

## 2. Gate G-Complexity Thresholds (Code & Structural Modes)

| Complexity Range (Code $CC$) | Structural Complexity Equivalent (Prose / Models) | Rating | Action Required |
| :---: | :--- | :---: | :--- |
| **$CC \in [1, 6]$** | Section nesting $\le 2$, single-purpose section, flat decision table ($\le 2$ conditions) | **Ideal (Green)** | Clean, focused, easily verifiable. Ready for review. |
| **$CC \in [7, 10]$** | Linear multi-step flow or schedule with explicit cross-reference index | **Acceptable Ceiling (Yellow)** | Permitted only if nesting depth $\le 2$ and flow is linear. |
| **$CC \in [11, 15]$** | Deeply nested sub-clauses (depth $3$), tangled assumptions, or multi-CTA copy | **Refactor Required (Orange)** | **Must decompose before PR merge.** Split into modular sub-sections or decision tables. |
| **$CC > 15$** | Circular cross-references, nesting depth $\ge 4$, or unverified spaghetti formulas | **Hard Merge Blocker (Red)** | **Gate G-Complexity FAILURE.** Blocks `gh pr merge` immediately. |

### Structural Complexity Mode Invariants (Prose, Specs, Financial Models, Research & Healthcare)
When auditing non-code deliverables (`--field product|marketing|finance|research|healthcare|gis`), evaluate:
1. **Section & Condition Nesting Depth $\le 2$**: Avoid deeply nested conditional prose ("If X, then if Y, unless Z..."). Flatten complex rules into explicit Markdown decision tables.
2. **Single Responsibility per File / Section**: Each file, schedule, or chapter addresses exactly one bounded concept, persona flow, channel playbook, financial statement, or hypothesis.
3. **Zero Circular Cross-References**: Dependencies between sections, schedules, or formulas must form a strict Directed Acyclic Graph (DAG).
4. **Clear Decision Tables & Transparent Assumptions**: Replace ambiguous prose branching or hardcoded magic numbers with tabular matrices and named input assumptions.

---

## 3. Five Proven Complexity-Reduction Patterns

### Pattern 1: Guard Clauses (Flatten Deep Nesting)
Replace nested `if { if { if { ... } } }` pyramids with early returns at the top of the function:
```typescript
// BEFORE (Nested, CC = 5, Depth = 3)
function processOrder(order?: Order): Result {
  if (order) {
    if (order.items.length > 0) {
      if (order.isPaid) {
        return fulfill(order);
      }
    }
  }
  return reject();
}

// AFTER (Flat Guard Clauses, CC = 4, Depth = 1)
function processOrder(order?: Order): Result {
  if (!order || order.items.length === 0 || !order.isPaid) {
    return reject();
  }
  return fulfill(order);
}
```

### Pattern 2: Extract Named Predicate Helpers
Move compound boolean expressions (`a && b || c && d`) into descriptive predicate functions (`isEligibleForRetry(req)`), reducing the caller's $CC$ and making the predicate independently unit-testable.

### Pattern 3: Replace Long `switch` / `if-else` Chains with Table / Map Dispatch (or Decision Matrices)
When branching selects data or a handler by key/type, replace an $N$-branch `switch` ($CC = N$) or nested prose rules with a static lookup table, dictionary ($CC = 2$), or structured decision table.

### Pattern 4: Separate Validation, Transformation, and I/O (or Inputs, Model, and Output Summary)
Split monolithic functions or schedules into three composable layers (`validate_request` / Assumptions, `build_payload` / Core Calculation, `execute_call` / Output Presentation), each with $CC \le 4$.

### Pattern 5: Polymorphism / Strategy Pattern
Replace repeated type-tag branching across multiple functions with a trait, interface, or strategy method implementation.
