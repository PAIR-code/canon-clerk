---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Special sentinel values with non-literal runtime semantics on a primitive parameter MUST be documented in the function's public doc comment.

Precedent: *Design by Contract* (Meyer); *Code Complete* (McConnell); Google Style Guides.
