---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Per *A Philosophy of Software Design* (Ousterhout, Ch. 13) and the Rust API Guidelines, public doc comments for functions returning ordered or sortable collections MUST state whether element ordering is guaranteed and by what criterion when callers depend on order.

Exception: Functions returning inherently unordered sets or maps MAY omit ordering documentation IFF the return type itself conveys unordered iteration semantics.
