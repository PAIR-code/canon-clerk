---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Code comments MUST describe the current code and its invariants rather than narrating diffs, edits, or historical changes (such as "Changed from X to Y" or "Removed old fallback").

Rationale: Per Google Style Guides and Git version-control hygiene, edit history belongs in commit logs; diff-narration comments immediately become stale clutter after merge.
