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
Unit tests MUST interact with and assert against public interfaces, and MUST NOT couple to private properties, internal helper functions, or execution sequences.

Exception: Pure mathematical or algorithmic calculation helpers MAY be tested directly IFF exported as deterministic pure units.

Rationale: Citing Gerard Meszaros (*xUnit Test Patterns: Overspecified Tests*), coupling tests to private state or internal call sequences causes test fragility, breaking benign refactors that preserve external behavior.

**Remediation:** Remove assertions on private members and spies on internal functions; drive verification strictly through public API inputs and outputs.
