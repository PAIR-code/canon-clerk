---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-quality
  - security
---
Inline static-analysis, linter, and security-scanner suppression directives MUST include a specific justification that is factually substantiated by the surrounding code.

Precedent: OWASP Code Review Guide; static-analysis governance practices.

Rationale: Deterministic linters can verify that a suppression comment exists but cannot verify whether its stated claim holds in the surrounding code; vacuous or inaccurate suppressions silently disable safety and security gates.
