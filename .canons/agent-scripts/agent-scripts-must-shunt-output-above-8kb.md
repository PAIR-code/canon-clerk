---
triggers:
  - ".agents/skills/**"
tags:
  - agent-scripts
---
Agent skill scripts invoking commands whose output can exceed 8KB MUST redirect output to a temporary file.

Rationale: Assistant subshell runners enforce strict stdout truncation caps (typically 8KB); shunting large payloads to temporary files prevents loss of critical diagnostic trails while permitting paginated inspection.
