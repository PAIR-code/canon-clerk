---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Per *A Philosophy of Software Design* (Ousterhout, Ch. 13) and Google Style Guides, public doc comments for functions returning raw numeric values representing physical, temporal, or scaled measurements MUST state the unit of measurement.

Exception: Functions returning raw numeric values MAY omit unit documentation IFF the unit is encoded in the function name or the returned value is a dimensionless count or ratio.
