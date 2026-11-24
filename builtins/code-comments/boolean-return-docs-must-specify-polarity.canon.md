---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Public doc comments for functions returning boolean values MUST state the condition under which the result is true.

Exception: Predicate functions MAY omit polarity documentation IFF the function name forms an unambiguous affirmative predicate (such as `is_empty` or `contains`).

Precedent: *A Philosophy of Software Design* (Ousterhout, Ch. 13); Google Style Guides.
