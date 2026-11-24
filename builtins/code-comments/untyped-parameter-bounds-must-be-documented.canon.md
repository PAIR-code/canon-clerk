---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Per *Design by Contract* (Meyer), *Code Complete* (McConnell), and Google Style Guides, runtime numeric or domain range bounds enforced on a primitive parameter outside its static type MUST be documented in the function's public doc comment.
