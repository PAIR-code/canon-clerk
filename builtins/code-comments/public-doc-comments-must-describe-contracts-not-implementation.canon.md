---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Following John Ousterhout (*A Philosophy of Software Design*, Ch. 13.4), Effective Go, and the Rust API Guidelines, public API doc comments MUST describe caller-facing contracts rather than private implementation mechanics.

Exception: Public doc comments MAY mention internal implementation details IFF they constitute part of the caller-observable behavioral, concurrency, or algorithmic complexity contract.
