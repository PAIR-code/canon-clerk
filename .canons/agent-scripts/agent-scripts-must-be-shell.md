---
triggers:
  - ".agents/skills/**"
tags:
  - agent-scripts
---
Agent skill scripts MUST be implemented exclusively in POSIX Bourne Shell (`*.sh`).

Rationale: AI coding assistants execute scripts across minimal environments without specialized runtimes; standardizing on POSIX shell guarantees predictable, dependency-light execution.
