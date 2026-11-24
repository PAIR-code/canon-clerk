---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Runtime numeric or domain range bounds enforced on a primitive parameter outside its static type MUST be documented in the function's public doc comment.

Precedent: *Design by Contract* (Meyer); *Code Complete* (McConnell); Google Style Guides.
