---
triggers:
  - "**/*.test.*"
  - "**/*.spec.*"
  - "**/*_test.*"
  - "**/test_*.py"
  - ".github/workflows/**"
inspect:
  - diff
tags:
  - testing
  - ci
  - monorepo
---
Non-hermetic integration test suites requiring external network access, ambient host credentials, or third-party service quotas MUST NOT execute as part of routine commit-stage verification or default pull request CI gates, and MUST reside in isolated execution tiers.

Precedent: Martin Fowler (*Continuous Integration*; *Continuous Delivery* commit stage).

Rationale: Routine verification gates must remain fast, deterministic, and executable offline; coupling pre-commit checks or pull request CI gates to live network endpoints causes false-positive failures on forks lacking secrets and introduces provider rate-limit volatility.
