---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Inline implementation comments MUST convey non-obvious intent, rationale, invariants, or trade-offs rather than mechanically restating what the adjacent syntax already expresses.

Exception: Inline comments MAY summarize adjacent syntax IFF acting as high-level phase headers in multi-step algorithms or translating dense regular expressions, bitmasks, or wire layouts.

Precedent: John Ousterhout (*A Philosophy of Software Design*, Ch. 13).
