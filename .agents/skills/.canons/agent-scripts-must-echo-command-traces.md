---
triggers:
  - "**/*.{sh,ts}"
inspect:
  - diff
tags:
  - agent-scripts
---
Agent skill scripts MUST echo their external commands when executed.

Rationale: AI coding assistants execute scripts within non-interactive shells; echoing command traces provides auditable evidence of execution, preventing assistants from losing confidence and reverting to manual commands.

**Remediation:** In shell scripts, enable execution tracing at the top of the script (e.g. `set -x`). In TypeScript or Node scripts, log invoked command strings to `stderr` or `stdout` immediately prior to spawning child processes.
