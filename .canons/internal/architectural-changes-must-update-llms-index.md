---
triggers:
  - "packages/**"
  - "specs/**"
  - "docs/architecture.md"
  - "SPEC.md"
inspect:
  - diff
  - pr_body
tags:
  - internal
  - repo-governance
---
Pull requests introducing or fundamentally altering architectural surfaces, core monorepo packages, or formal specifications MUST update `llms.txt` to reflect the new entry point.

Rationale: AI coding assistants rely on `llms.txt` for cold-read repository orientation; omission of newly introduced architectural boundaries leads to stale navigation models.

**Remediation:** Identify the new architectural surface introduced in this PR and suggest an entry under the appropriate section in `llms.txt` (or introduce a new section if a new architectural pillar is being established).
