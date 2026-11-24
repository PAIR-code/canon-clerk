---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Public doc comments for functions returning semantically ambiguous types (`Option`/`null`, `bool`, raw numeric units, or collections with ordering guarantees) MUST clarify what `None`/`null` signifies, what units a numeric value represents, or whether element ordering is guaranteed.

Exception: Public functions MAY omit return-semantics documentation IFF the function name and type signature make the return value unambiguous (such as `len() -> usize` or `is_empty() -> bool`).

Rationale: Per *A Philosophy of Software Design* (Ousterhout, Ch. 13) and the Rust API Guidelines, bare return types leave callers guessing whether `None` means "not found", "empty", or "disabled", or whether numeric values are milliseconds or seconds.
