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
---
When live service integration tests fail due to missing ambient credentials or unreachable endpoints, failure diagnostics MUST output actionable remediation instructions.

Precedent: Command Line Interface Guidelines (clig.dev §Errors).

Rationale: Providing specific recovery steps (such as configuration file paths, sample snippets, environment variable names, or key generation URLs) eliminates triage guesswork and accelerates developer onboarding.
