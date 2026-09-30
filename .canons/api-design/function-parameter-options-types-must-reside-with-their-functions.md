---
triggers:
  - "**/src/**/*.ts"
inspect:
  - diff
tags:
  - api-design
  - typescript
---
An Options type configuring a specific function parameter MUST be declared and exported from that function's module rather than an external types module.

Rationale: Citing the Common Closure Principle (Martin), types that change together belong together.
