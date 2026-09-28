---
triggers:
  - "**/*.{sh,ts}"
inspect:
  - diff
tags:
  - agent-scripts
---
Agent skill scripts that invoke commands with potentially unbounded or high-volume output MUST redirect output to a temporary file when exceeding 8KB.

Rationale: Assistant subshell runners enforce strict stdout truncation limits (typically 8KB); shunting voluminous payloads prevents loss of critical diagnostic trails while permitting paginated inspection.

**Guidance:** Buffer or measure command output volume. If output exceeds 8KB, redirect the complete stream to a temporary file (e.g. in `/tmp/`) and print the resulting file path alongside a compact summary or failure slice.
