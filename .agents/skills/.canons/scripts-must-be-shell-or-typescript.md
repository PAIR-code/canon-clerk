---
governs:
  - ".agents/skills/**"
inspect:
  - diff
  - pr_body
tags:
  - internal
  - agent-scripts
---
Agent skill scripts MUST be implemented exclusively in POSIX Bourne Shell (`*.sh`) or TypeScript/Node (`*.ts`). Operational scripts requiring external interpreters (such as Python, Ruby, or Perl) are forbidden.

Rationale: AI coding assistants execute scripts across minimal environments without specialized runtimes; standardizing on POSIX shell and TypeScript/Node guarantees predictable, dependency-light execution.

**Guidance:** If this file is an operational script invoked by an agent skill in `SKILL.md`, re-implement it in POSIX `sh` or TypeScript. If it is a passive asset, fixture, or template, ensure its passive purpose is clear from its location or documentation.
