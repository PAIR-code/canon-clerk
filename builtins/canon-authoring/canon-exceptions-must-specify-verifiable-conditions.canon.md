---
triggers:
  - "**/*.canon.md"
inspect:
  - diff
tags:
  - canon-authoring
---
Canon `Exception` clauses MUST specify objective, verifiable conditions using RFC 2119 biconditional `MAY ... IFF ...` syntax rather than discretionary or subjective exemptions.

Precedent: RFC 2119; Canon Format Specification (§5).

Rationale: The AI evaluator screens exceptions for semantic sufficiency; subjective exemptions introduce evaluation nondeterminism and prompt drift.

**Remediation:** Formulate exceptions with concrete, falsifiable evidentiary requirements, such as benchmark metrics, CVE citations, or explicit inline architectural comments.
