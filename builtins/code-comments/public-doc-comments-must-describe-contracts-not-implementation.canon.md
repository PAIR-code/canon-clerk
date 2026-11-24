---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Public API doc comments MUST describe caller-facing contracts rather than private implementation mechanics.

Exception: Public doc comments MAY mention internal implementation details IFF they constitute part of the caller-observable behavioral, concurrency, or algorithmic complexity contract.

Precedent: John Ousterhout (*A Philosophy of Software Design*, Ch. 13.4); *Design by Contract* (Meyer).
