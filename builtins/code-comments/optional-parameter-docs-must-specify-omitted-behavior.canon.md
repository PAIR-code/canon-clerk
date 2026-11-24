---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Public doc comments for optional parameters (`Option<T>`, `T | undefined`, `x=None`, or nullable fields) MUST state the runtime behavior when the argument is omitted or `None`/`null`.

Exception: Optional parameters MAY omit explicit default-behavior documentation IFF the signature declares a self-evident literal default value (such as `retries: usize = 3` or `verbose: bool = false`).

Rationale: Following the Google API Design Guide (AIP-134, AIP-140), Rust API Guidelines, and *A Philosophy of Software Design* (Ousterhout), an optional type communicates only that omission is syntactically permitted, not whether omission disables a feature, inherits a fallback, or applies a default threshold.
