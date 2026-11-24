---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Per *Design by Contract* (Meyer), *Code Complete* (McConnell), and Google Style Guides, runtime domain bounds or sentinel meanings enforced on a primitive parameter outside its static type MUST be documented in the function's public doc comment.
