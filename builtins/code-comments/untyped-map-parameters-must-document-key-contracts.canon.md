---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Following PEP 257, the Google Python Style Guide, and TSDoc conventions, public doc comments for untyped map, dictionary, or variadic keyword parameters MUST document the recognized keys, expected value types, and unknown-key handling semantics.

Exception: Transparent passthrough wrappers MAY omit enumerating individual keys IFF the doc comment explicitly references the target API to which arguments are forwarded untouched.
