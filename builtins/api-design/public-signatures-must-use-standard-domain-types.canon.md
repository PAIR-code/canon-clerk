---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - api-design
---
Public function signatures MUST use standard-library domain types (such as `Duration`, `Instant`/`DateTime`, `Path`/`PathBuf`, `Url`/`Uri`, or closed enums) rather than raw primitives (`i64`, `u64`, `String`, or `bool` flags) when the target language's standard library provides the domain type.

Exception: Wire-protocol, FFI, or CLI serialization boundary types MAY use raw primitives IFF the external schema or ABI requires primitive representations.

Rationale: Following Alexis King ("Parse, Don't Validate"), Yaron Minsky ("Make Illegal States Unrepresentable"), Martin Fowler (*Refactoring*, Primitive Obsession), and the Rust API Guidelines (`C-NEWTYPE`), raw primitives discard unit and domain invariants at the API boundary and force repeated defensive validation.
