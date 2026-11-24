---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Public doc comments for untyped map, dictionary, or variadic keyword parameters MUST document the recognized keys, expected value types, and unknown-key handling semantics.

Exception: Transparent passthrough wrappers MAY omit enumerating individual keys IFF the doc comment explicitly references the target API to which arguments are forwarded untouched.

Precedent: *Design by Contract* (Meyer); *A Philosophy of Software Design* (Ousterhout); Google Style Guides.
