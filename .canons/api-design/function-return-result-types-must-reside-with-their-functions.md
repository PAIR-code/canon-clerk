---
triggers:
  - "**/src/**/*.ts"
inspect:
  - diff
tags:
  - api-design
  - typescript
---
A bespoke Result or Output type representing the return value of a specific function MUST be declared and exported from that function's module rather than an external types module.

Exception: Shared domain entities and ubiquitous value objects reused across multiple independent operations are exempt.

Rationale: Citing the Common Closure Principle (Martin), a function's contract (both parameter options and operation results) changes together with its implementation and belongs together. Isolating operation results in external type files produces asymmetric, fractured function signatures and creates unorganized types sinkholes.
