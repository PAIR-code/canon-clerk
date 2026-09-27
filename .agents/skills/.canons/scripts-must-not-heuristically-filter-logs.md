---
paths:
  - "**/*.{sh,ts}"
inspect:
  - diff
tags:
  - agent-scripts
---
Agent skill scripts triaging diagnostic or failure outputs MUST NOT use heuristic filters or error-guessing regular expressions to curate failure slices.

Rationale: Heuristic regex filtering strips surrounding context, introduces false positives, and conceals unexpected failure modes, degrading assistant confidence and forcing models to re-run raw commands.

**Guidance:** Persist unedited command logs to `/tmp/` and display an authentic tail (e.g. `tail -n 60`), allowing the executing assistant to perform semantic diagnosis from surrounding context.
