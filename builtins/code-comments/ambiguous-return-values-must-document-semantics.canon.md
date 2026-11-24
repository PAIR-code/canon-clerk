---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Per *A Philosophy of Software Design* (Ousterhout, Ch. 13) and the Rust API Guidelines, public doc comments for functions returning semantically ambiguous types MUST clarify nullability semantics, numeric units, boolean polarity, or collection ordering guarantees when not self-evident from the signature.

Exception: Public functions MAY omit return-semantics documentation IFF the function name and type signature make the return value unambiguous (such as `len() -> usize` or `is_empty() -> bool`).
