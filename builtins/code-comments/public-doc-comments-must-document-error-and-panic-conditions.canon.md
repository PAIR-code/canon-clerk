---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Public doc comments for fallible or panicking functions MUST document the observable error, exception, or panic conditions raised by the implementation.

Exception: Fallible functions MAY omit enumerating propagated errors IFF they perform trivial pass-through propagation whose failure mode is self-evident from the wrapped operation or structured error type.

Rationale: Following the Rust API Guidelines (`C-FAILURE`), *Effective Java* (Bloch, Item 74), and PEP 257, callers cannot implement sound error recovery or uphold non-panicking preconditions without knowing which inputs trigger failures.
