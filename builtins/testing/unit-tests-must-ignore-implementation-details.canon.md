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
Unit tests MUST verify behavior strictly through public interfaces rather than coupling to private properties, internal helper functions, or execution sequences.

Exception: Pure mathematical or algorithmic calculation helpers MAY be tested directly IFF exported as deterministic pure units.

Precedent: Gerard Meszaros (*xUnit Test Patterns*, "Overspecified Tests").

Rationale: Coupling tests to private state or internal call sequences causes test fragility, breaking benign refactors that preserve external behavior.
