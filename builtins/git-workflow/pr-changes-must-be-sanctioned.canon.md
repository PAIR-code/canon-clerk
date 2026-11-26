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
Pull request diffs MUST only implement changes sanctioned by their motivating issue and pull request description.

Exception: A pull request MAY introduce or update project canons not explicitly enumerated in the motivating issue IFF the canons codify invariants directly governing the files, subsystems, or specifications modified in the pull request, and are documented in the pull request summary.

Precedent: Git atomic commit practices; change isolation principles.

Rationale: Bundling out-of-band changes discards situational discovery context, complicates code review, breaks Git bisectability, and distorts squash-merge changelogs.

**Remediation:** Shunt unrelated work to a new tracking issue (recommended) OR execute an upstream chase to expand the motivating issue scope.
