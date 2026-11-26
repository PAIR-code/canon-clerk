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
  - code-quality
---
Unit tests MUST assert against observable return values, state mutations, or contractually thrown errors of the system under test.

Precedent: Kent Beck (*Test-Driven Development*).

Rationale: Executing code without asserting on outputs or state mutations detects unhandled crashes but permits silent functional regressions.
