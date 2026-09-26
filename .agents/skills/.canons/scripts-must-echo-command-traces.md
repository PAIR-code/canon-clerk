---
paths:
  - "**/*.{sh,ts}"
inspect:
  - diff
---
Agent skill scripts MUST echo their external commands when executed.

Rationale: AI coding assistants execute companion scripts within non-interactive shells. When scripts run external processes silently or conceal their operations, assistants cannot verify execution traces and frequently lose confidence, falling back to manually running raw commands. Echoing command traces (e.g. `set -x` in shell scripts or explicit command logging before process execution in TypeScript) provides transparent, auditable evidence of every operation.

**Guidance:** In shell scripts, enable execution tracing at the top of the script (e.g. `set -x`). In TypeScript or Node scripts, log invoked command strings to `stderr` or `stdout` immediately prior to spawning or executing child processes.
