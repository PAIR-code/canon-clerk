---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - architecture
---
Domain modules and library functions MUST accept narrow, module-scoped option structures containing only the parameters they consume, rather than accepting or traversing an application-wide configuration container.

Exception: Composition roots, CLI command routers, and configuration loaders MAY inspect the application-wide configuration container IFF projecting narrow option structures for downstream modules.

Precedent: Yourdon & Constantine (*Structured Design*); Law of Demeter.

Rationale: Stamp coupling binds independent modules and their unit tests to unrelated configuration schema changes.
