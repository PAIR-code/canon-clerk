---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - architecture
---
Domain engines and core library modules MUST NOT read user home directories, host profile stores (`~/.config`), or ambient process state outside the workspace envelope.

Exception: CLI adapters, configuration loaders, and application entrypoints MAY read host configuration files and environment variables IFF passing the resolved values into domain engines as explicit in-memory parameters.

Precedent: Hexagonal Architecture (Cockburn); Clean Architecture (Martin).

Rationale: Isolating domain engines from host filesystem state guarantees deterministic execution across hermetic tests, sandboxes, and containerized CI runners.
