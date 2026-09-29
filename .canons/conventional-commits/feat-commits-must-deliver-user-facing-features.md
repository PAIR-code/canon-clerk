---
triggers:
  - "**"
inspect:
  - diff
  - pr_title
  - commit_messages
tags:
  - conventional-commits
---
Pull request titles and commit messages using the `feat` Conventional Commit type MUST describe changes that introduce new user-facing functionality. Pull requests and commits that introduce internal changes (such as developer tooling, agent skills, CI automation, test harnesses, or refactors) MUST NOT use the `feat` type.

Rationale: The `feat` type triggers a SemVer minor release and publishes user-facing release notes; marking internal changes as features generates spurious releases and misleads consumers.

**Remediation:** Re-title the pull request or adjust commit messages to use an appropriate non-releasing type (such as `chore:`, `ci:`, `build:`, or `refactor:`) following the surface-to-prefix mappings in `docs/conventional-commits.md#2-surface-to-prefix-mapping`.
