---
triggers:
  - "**"
inspect:
  - diff
  - pr_body
  - linked_issues
tags:
  - git-workflow
---
A pull request MUST only implement changes sanctioned by its motivating issue and PR description. Out-of-band changes, drive-by refactorings, or unrelated improvements MUST be omitted.

Exception: A pull request MAY introduce or update project canons not explicitly enumerated in the motivating issue IFF the canons codify invariants directly governing the files, subsystems, or specifications modified in the pull request, and are documented in the pull request summary.

Rationale: In line with Git atomic commit practices and standard change isolation, bundling out-of-band changes discards situational discovery context, complicates code review, breaks Git bisectability, and distorts squash-merge changelogs.

**Guidance:** Shunt unrelated work to a new tracking issue (recommended) OR execute an upstream chase to expand the motivating issue scope.
