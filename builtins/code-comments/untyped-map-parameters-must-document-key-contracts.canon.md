---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
When a public function accepts an untyped map, dictionary, or variadic keyword arguments (`**kwargs`, `Record<string, unknown>`, `map[string]any`), its doc comment MUST document the recognized keys, expected value types, and handling of unknown keys (ignored, rejected, or forwarded).

Exception: Transparent passthrough wrappers MAY omit enumerating individual keys IFF the doc comment explicitly references the target API to which arguments are forwarded untouched.

Rationale: Following PEP 257, the Google Python Style Guide, and TSDoc conventions, untyped mapping parameters hide their input schema from static tooling and force callers to inspect implementation code.
