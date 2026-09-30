---
triggers:
  - ".agents/skills/**"
inspect:
  - diff
  - pr_body
tags:
  - internal
  - agent-scripts
---
Agent skill scripts under `.agents/skills/**` MUST be implemented exclusively in POSIX Bourne Shell (`*.sh`).

Rationale: AI coding assistants execute scripts across minimal environments without specialized runtimes; standardizing on POSIX shell guarantees predictable, dependency-light execution.
