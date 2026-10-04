---
triggers:
  - "**/*integration*.*"
  - "**/*integration*/**"
  - "**/*.integration.test.*"
  - "**/*.integration.spec.*"
  - "**/*_integration_test.*"
  - "**/test_*integration*.py"
inspect:
  - diff
tags:
  - testing
  - ci
  - ergonomics
---
Live service integration tests MUST fail by default when required external credentials or endpoints are absent, rather than silently skipping.

Exception: Tests MAY skip execution cleanly IFF an explicit bypass mechanism (such as an opt-out environment variable or runner filter flag) is enabled.

Rationale: Citing Google Testing Blog (*Silent Failures and Anti-Patterns*), silently skipping unconfigured tests creates false confidence in continuous integration environments by passing green when functionality was never exercised; failing by default ensures missing prerequisites are detected immediately.
