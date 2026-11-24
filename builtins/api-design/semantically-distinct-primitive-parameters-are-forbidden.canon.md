---
triggers:
  - "**/*.rs"
  - "**/*.ts"
  - "**/*.tsx"
  - "**/*.go"
  - "**/*.java"
  - "**/*.kt"
  - "**/*.cs"
  - "**/*.c"
  - "**/*.cpp"
  - "**/*.h"
  - "**/*.hpp"
inspect:
  - diff
tags:
  - api-design
---
In accordance with the Rust API Guidelines (`C-NEWTYPE`, `C-CUSTOM-TYPE`), Domain-Driven Design Value Objects (Evans), and Martin Fowler's Primitive Obsession, positional parameters of functions, methods, and constructors in statically typed languages MUST NOT repeat the same scalar primitive type when representing distinct domain concepts or units.

Exception: Pure mathematical, comparison, coordinate, or text-manipulation utilities MAY accept multiple positional parameters of the same primitive type IFF the operands share an identical semantic unit (such as `min(a, b)`, `split(haystack, delimiter)`, or `(start_offset, end_offset)` bounds).

**Remediation:** Disambiguate parameters at the type level using distinct newtype wrappers, branded types, domain-specific enums, or a parameter struct with named fields.
