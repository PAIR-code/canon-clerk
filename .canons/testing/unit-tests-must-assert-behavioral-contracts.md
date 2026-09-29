---
triggers:
  - "**/*.test.*"
  - "**/*.spec.*"
  - "**/*_test.*"
  - "**/test_*.py"
inspect:
  - diff
tags:
  - testing
  - test-design
  - code-quality
---
Unit tests MUST assert against observable return values, state mutations, or contractually thrown errors of the system under test.

Rationale: Following Kent Beck (*Test-Driven Development*), tests verify observable behavioral contracts; executing code without asserting on outputs or state mutations detects unhandled crashes but permits silent functional regressions.

**Remediation:** Assert against public return values, state mutations, or expected error instances rather than asserting trivial truthy flags or omitting assertions.
