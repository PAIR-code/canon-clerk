---
paths:
  - ".agents/skills/**"
inspect:
  - diff
  - pr_body
---
Companion scripts intended for AI agent execution MUST be implemented exclusively in POSIX Bourne Shell (`*.sh`) or TypeScript/Node (`*.ts`). Operational scripts requiring external interpreters (such as Python, Ruby, or Perl) are forbidden.

Rationale: AI coding assistants execute companion scripts across diverse, minimal environments without guarantees of specialized language runtimes. Standardizing operational scripts strictly on standard POSIX shell and TypeScript/Node ensures predictable, dependency-light execution. Passive assets, reference fixtures, and code generation templates are exempt from this requirement.

**Guidance:** If this file is an operational script invoked by the agent in `SKILL.md`, re-implement it in POSIX `sh` or TypeScript. If it is a passive asset, fixture, or template, ensure its passive purpose is clear from its location or documentation.
