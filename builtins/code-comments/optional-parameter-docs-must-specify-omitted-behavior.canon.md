---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Public doc comments for optional or nullable parameters MUST state the runtime behavior when the argument is omitted or absent.

Exception: Optional parameters MAY omit explicit default-behavior documentation IFF the signature declares a self-evident literal default value (such as `retries: usize = 3` or `verbose: bool = false`).

Precedent: Google API Design Guide (AIP-134, AIP-140); *A Philosophy of Software Design* (Ousterhout).
