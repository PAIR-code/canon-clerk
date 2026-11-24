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
Functions, methods, and constructors in statically typed languages MUST NOT accept two or more positional parameters of the same primitive type (string, numeric, or boolean) when those parameters represent distinct domain concepts or units.

Exception: Pure mathematical, comparison, coordinate, or text-manipulation utilities MAY accept multiple positional parameters of the same primitive type IFF the operands share an identical semantic unit (such as `min(a, b)`, `split(haystack, delimiter)`, or `(start_offset, end_offset)` bounds).

Rationale: In accordance with the Rust API Guidelines (`C-NEWTYPE`, `C-CUSTOM-TYPE`), Domain-Driven Design Value Objects (Evans), and Martin Fowler's Primitive Obsession, positional parameters sharing a primitive representation bypass static type checking against accidental argument transposition at call sites.

**Remediation:** Disambiguate parameters at the type level using distinct newtype wrappers, branded types, domain-specific enums, or a parameter struct with named fields.
