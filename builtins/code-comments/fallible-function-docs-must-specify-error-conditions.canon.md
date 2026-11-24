---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Following the Rust API Guidelines (`C-FAILURE`), *Effective Java* (Bloch, Item 74), and PEP 257, public doc comments for fallible functions MUST document the observable error or exception conditions raised by the implementation.

Exception: Fallible functions MAY omit enumerating propagated errors IFF returning a structured error type directly forwarded from a single wrapped fallible call.
