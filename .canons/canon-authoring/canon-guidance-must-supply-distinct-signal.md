---
triggers:
  - "**/.canons/**/*.md"
inspect:
  - diff
tags:
  - canon-authoring
---
When declared, a `Guidance` directive MUST provide concrete, actionable remediation steps or references beyond restating compliance with the invariant rule.

Rationale: Following the Canon Format Specification (SPEC.md §5), guidance directs contributor action upon failure; tautological instructions leave contributors unassisted while inflating prompt context.

**Guidance:** Direct the contributor to specific documentation anchors, CLI commands, templates, or refactoring strategies, or omit the directive if remediation is self-explanatory.
