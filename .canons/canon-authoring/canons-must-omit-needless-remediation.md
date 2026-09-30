---
triggers:
  - "**/.canons/**/*.md"
tags:
  - canon-authoring
---
A `Remediation` directive MUST be omitted when the corrective action is self-evident from the invariant statement or exception clauses.

Rationale: Per SPEC.md §5, `Remediation` is reserved for actionable pointers (templates, documentation anchors, or refactoring commands); restating compliance as "do what the rule says" adds zero assistive guidance while consuming context.
