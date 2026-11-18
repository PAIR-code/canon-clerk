---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - architecture
  - modularity
---
Domain modules and library functions MUST accept narrow, module-scoped option structures containing only the parameters they consume, rather than accepting or traversing an application-wide configuration container.

Exception: Composition roots, CLI command routers, and configuration loaders MAY inspect the application-wide configuration container IFF projecting narrow option structures for downstream modules.

Rationale: Citing Yourdon & Constantine (*Structured Design*) and the Law of Demeter, stamp coupling binds independent modules and their unit tests to unrelated configuration schema changes.
