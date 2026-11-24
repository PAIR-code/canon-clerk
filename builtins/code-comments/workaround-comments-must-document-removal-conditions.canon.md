---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Workaround and `HACK` comments MUST identify the upstream defect, platform quirk, or external constraint being worked around and the condition under which the workaround can be removed.

Rationale: Following Chesterton's Fence and Google Style Guides, undocumented workarounds cannot be safely retired when upstream dependencies or platform constraints evolve.
