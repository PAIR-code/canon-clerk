---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Public API doc comments MUST describe caller-facing contracts (inputs, outputs, errors, and behavioral guarantees) rather than private implementation mechanics.

Exception: Public doc comments MAY mention internal implementation details IFF they constitute part of the caller-observable behavioral, concurrency, or algorithmic complexity contract.

Rationale: Following John Ousterhout (*A Philosophy of Software Design*, Ch. 13.4), Effective Go, and the Rust API Guidelines, leaking private mechanics into interface documentation couples callers to internals and causes documentation drift during refactors.
