---
triggers:
  - "**/.canons/**/*.md"
inspect:
  - diff
tags:
  - canon-authoring
---
Canon `Exception` clauses MUST specify objective, verifiable conditions using RFC 2119 biconditional keywords (such as `MAY ... IFF ...`) rather than discretionary or subjective exemptions.

Rationale: Following the Canon Format Specification (SPEC.md §5), the AI evaluator screens exceptions for semantic sufficiency; subjective exemptions introduce evaluation nondeterminism and prompt drift.

**Remediation:** Formulate exceptions with concrete, falsifiable evidentiary requirements, such as benchmark metrics, CVE citations, or explicit inline architectural comments.
