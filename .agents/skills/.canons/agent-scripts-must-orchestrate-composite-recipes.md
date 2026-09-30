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
Agent skill scripts MUST orchestrate multiple CLI commands commonly executed together into a composite recipe.

Rationale: AI coding assistants natively know how to execute individual toolchain commands; dedicated companion scripts are justified only when sequencing multi-step workflows, eliminating repetitive command chains, or orchestrating interdependent CLI operations.
