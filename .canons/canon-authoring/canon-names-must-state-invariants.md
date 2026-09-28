---
governs:
  - "**/.canons/**/*.md"
inspect:
  - diff
tags:
  - canon-authoring
---
Each canon file name MUST state a testable invariant or policy rather than a passive topic or category.

Rationale: Naming canons after their invariants ensures review reports and CI check lists immediately communicate each expectation being verified.

**Guidance:** Rename the file to assert the specific policy or condition being enforced (e.g. `prs-must-document-new-features.md` rather than `documentation.md`).
