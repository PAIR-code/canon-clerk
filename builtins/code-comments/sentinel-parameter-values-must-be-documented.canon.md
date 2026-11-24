---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Per *Design by Contract* (Meyer), *Code Complete* (McConnell), and Google Style Guides, special sentinel values with non-literal runtime semantics on a primitive parameter MUST be documented in the function's public doc comment.
