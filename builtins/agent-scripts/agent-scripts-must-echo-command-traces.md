---
triggers:
  - ".agents/skills/**"
tags:
  - agent-scripts
---
Agent skill scripts MUST echo their external commands when executed.

Rationale: AI coding assistants execute scripts within non-interactive shells; echoing command traces (e.g. `set -x`) provides auditable evidence of execution, preventing assistants from losing confidence and reverting to manual commands.
