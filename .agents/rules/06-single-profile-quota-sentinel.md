# Reference 06: Single-Profile Operation, Quota Sentinel & Gemini Flash Optimization

## 1. Why This Plugin Targets the Gemini Flash Family

The Divide-and-Conquer methodology trades **token volume** for **engineering velocity and defect-free quality**:
- $K$ Unit Implementers run simultaneously in parallel worktrees.
- Multiple independent Code Reviewing Councils audit every unit, patch, story, and module.
- Iterative `patch-implementer` loops automatically remediate any complexity or security finding without human intervention.

Because this architecture executes many concurrent agent turns and full-context reviews, it **consumes a substantial number of tokens**. It is specifically designed and tuned for the **Gemini Flash family** (e.g., `gemini-2.5-flash` / `gemini-3-flash`), which provides the ideal balance of high reasoning speed, large context capacity, and cost/token efficiency for parallel multi-agent swarms.

## 2. Single-Profile Operation & Autonomous `quota-sentinel`

This plugin operates on **a single profile at a time**. To protect long-running autonomous builds against API rate limits or quota exhaustion:

1. **Continuous Monitoring**:
   - The `quota-sentinel` subagent monitors API responses for `429` / `RESOURCE_EXHAUSTED` signals or remaining quota dropping to **1%**.
2. **Graceful Pause & Checkpoint Flush**:
   - When triggered, `quota-sentinel` alerts the active supervisor to pause new subagent launches and flushes current worktree/PR states to `.agents/checkpoints/<module-id>/checkpoint.json`.
3. **Scheduled Wakeup & Idempotent Resume**:
   - A `schedule` timer waits until the quota window resets, then resumes execution by reconciling `checkpoint.json` with `git worktree list` and `gh pr list --state all` so zero tasks are duplicated.
