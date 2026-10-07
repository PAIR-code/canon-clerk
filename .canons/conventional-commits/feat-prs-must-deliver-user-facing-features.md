---
triggers:
  - "**"
inspect:
  - diff
  - pr_title
tags:
  - conventional-commits
---
Pull request titles using the `feat` Conventional Commit type MUST describe changes that introduce new user-facing functionality. Pull requests that only introduce internal changes (such as developer tooling, agent skills, CI automation, test harnesses, or refactors) MUST NOT use the `feat` type.

Rationale: The `feat` type triggers a SemVer minor release and publishes user-facing release notes; marking internal changes as features generates spurious releases and misleads consumers.
