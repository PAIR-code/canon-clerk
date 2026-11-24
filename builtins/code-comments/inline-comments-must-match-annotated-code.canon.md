---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Inline implementation comments describing control flow, behavior, edge cases, or invariants MUST factually match the code they annotate.

Rationale: Per *Code Complete* (McConnell), *The Pragmatic Programmer* (Hunt & Thomas), and Google Style Guides, stale or inaccurate inline comments actively mislead readers and AI agents during maintenance and debugging.
