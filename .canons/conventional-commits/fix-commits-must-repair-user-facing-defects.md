---
governs:
  - "**"
inspect:
  - diff
  - pr_title
  - commit_messages
tags:
  - conventional-commits
---
Pull request titles and commit messages using the `fix` Conventional Commit type MUST describe changes that repair a defect in existing user-facing functionality. Pull requests and commits that repair internal developer tools, build pipelines, broken tests, or non-production scripts MUST NOT use the `fix` type.

Rationale: The `fix` type triggers a SemVer patch release and publishes user-facing release notes; repairing internal developer infrastructure does not warrant a public release bump.

**Guidance:** Re-title the pull request or adjust commit messages to use an appropriate non-releasing type (such as `chore:`, `ci:`, `build:`, or `test:`) following the surface-to-prefix mappings in `docs/conventional-commits.md#2-surface-to-prefix-mapping`.
