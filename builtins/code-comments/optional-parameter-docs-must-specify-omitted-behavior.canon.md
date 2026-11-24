---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Following the Google API Design Guide (AIP-134, AIP-140), Rust API Guidelines, and *A Philosophy of Software Design* (Ousterhout), public doc comments for optional or nullable parameters MUST state the runtime behavior when the argument is omitted or absent.

Exception: Optional parameters MAY omit explicit default-behavior documentation IFF the signature declares a self-evident literal default value (such as `retries: usize = 3` or `verbose: bool = false`).
