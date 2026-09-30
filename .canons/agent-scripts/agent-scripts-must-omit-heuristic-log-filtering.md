---
triggers:
  - ".agents/skills/**"
tags:
  - agent-scripts
---
Agent skill scripts MUST NOT use heuristic regular expressions or error-guessing filters to truncate diagnostic outputs.

Rationale: Heuristic log filtering conceals unexpected failure modes and context; assistants require authentic output streams or persisted temp files for accurate diagnosis.
