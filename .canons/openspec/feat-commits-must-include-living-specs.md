---
triggers:
  - "**/src/**"
inspect:
  - diff
  - pr_title
  - commit_messages
tags:
  - openspec
  - conventional-commits
---
Pull requests introducing new user-facing functionality (`feat`) MUST add or update living specifications in `openspec/specs/`.

Rationale: Following OpenSpec and Conventional Commits 1.0.0 (SemVer MINOR), every user-facing feature expands the system's public contract. Under Living Documentation principles, shipping functionality without landing corresponding specifications causes immediate specification rot and degrades AI coding agent navigation models.

**Guidance:** Include the corresponding specification changes under `openspec/specs/<capability>/spec.md` (promoted from an OpenSpec change proposal) declaring the requirements and scenarios introduced by this feature.
