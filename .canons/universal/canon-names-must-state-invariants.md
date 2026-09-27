---
paths:
  - "**/.canons/**/*.md"
inspect:
  - diff
tags:
  - universal
  - canon-authoring
---
Each canon file name MUST state a testable invariant or policy rather than a passive topic or category.

Rationale: Because Canon Clerk derives check run titles and human-readable identifiers directly from file stems, naming canons after their invariants ensures review reports and CI check lists immediately communicate the expectation being verified.

**Guidance:** If feasible, suggest an alternative file name that states the underlying invariant rather than a topical noun phrase (e.g., recommend `prs-must-document-new-features.md` instead of `documentation.md`).
