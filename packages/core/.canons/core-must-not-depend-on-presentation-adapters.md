---
inspect:
  - diff
tags:
  - internal
  - architecture
  - core
---
Code in `@canon-clerk/core` MUST NOT import or declare dependencies on presentation adapters, including `@canon-clerk/cli`, `@canon-clerk/action`, or runner-specific libraries such as `@actions/core` or `@actions/github`.

Rationale: Following the Dependency Inversion Principle (SOLID DIP) and Clean Architecture Dependency Rule, high-level domain policies must never depend on delivery mechanisms; referencing adapters inverts architectural layering and creates circular coupling.

**Guidance:** Keep `@canon-clerk/core` independent of its consumers. If core needs input from a presentation environment (such as runner context or user configuration), accept those inputs via method arguments or dependency injection rather than importing adapter packages.
