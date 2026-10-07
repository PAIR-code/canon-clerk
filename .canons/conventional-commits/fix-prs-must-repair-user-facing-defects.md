---
triggers:
  - "**"
inspect:
  - diff
  - pr_title
tags:
  - conventional-commits
---
Pull request titles using the `fix` Conventional Commit type MUST describe changes that repair a defect in existing user-facing functionality. Pull requests that repair internal developer tools, build pipelines, broken tests, or non-production scripts MUST NOT use the `fix` type.

Rationale: The `fix` type triggers a SemVer patch release and publishes user-facing release notes; repairing internal developer infrastructure does not warrant a public release bump.
