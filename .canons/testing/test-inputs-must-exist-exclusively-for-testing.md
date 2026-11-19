---
triggers:
  - "**/src/**"
  - "**/tests/**"
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
Data and file inputs supplied to unit and integration tests MUST exist solely to exercise the system under test, serving no other build, runtime, documentation, or governance role in the repository.

Exception: Repository-conformance tests MAY inspect live project files as the subject under test IFF enforcing a repository-wide structural invariant for which no standalone CLI linter or CI check exists.

Rationale: Citing Gerard Meszaros (*xUnit Test Patterns*: *Shared Fixture* and *Fresh Fixture*), dual-purpose inputs couple behavioral verification of the system under test to unrelated repository edits—conflating code defects with content changes, amplifying failure blast radius across test suites, and scaling test latency with repository size.
