---
governs:
  - "**"
inspect:
  - diff
  - pr_body
  - linked_issues
tags:
  - git-workflow
---
A pull request MUST only implement changes sanctioned by its motivating issue and PR description. Out-of-band changes, drive-by refactorings, or unrelated improvements MUST be shunted to separate follow-up issues OR folded into the current workstream by deliberate expansion of the mandate via upstream chase to the motivating issue text.

Rationale: Bundling out-of-band changes discards situational discovery context, complicates code review, breaks Git bisectability, and distorts squash-merge changelogs.

**Guidance:** If encountering an unrelated defect, missing canon, or cleanup opportunity:
1. **Shunt it (Recommended):** File a new tracking issue documenting the problem and leave the current PR focused on its original scope.
2. **Upstream Chase:** If the change is strictly necessary for the current task to land, update the motivating issue and PR description to document the expanded scope before committing.
