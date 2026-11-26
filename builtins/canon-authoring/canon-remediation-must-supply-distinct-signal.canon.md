---
triggers:
  - "**/*.canon.md"
inspect:
  - diff
tags:
  - canon-authoring
---
When declared, a `Remediation` directive MUST provide concrete, actionable remediation steps or references beyond restating compliance with the invariant rule.

Precedent: Canon Format Specification (§5).

Rationale: Remediation directs contributor action upon failure; tautological instructions leave contributors unassisted while inflating prompt context.

**Remediation:** Direct the contributor to specific documentation anchors, CLI commands, templates, or refactoring strategies, or omit the directive if remediation is self-explanatory.
