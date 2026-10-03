---
triggers:
  - "packages/core/**"
tags:
  - internal
  - architecture
  - modularity
---
Reusable domain evaluation engines and core library packages MUST NOT probe user home directories, ambient host profile stores (`~/.config`), or global system state outside the workspace envelope; all host operating system interactions MUST be mediated by presentation adapters or configuration packages.

Rationale: Following Hexagonal Architecture (Cockburn) and Clean Architecture (Martin) Ports and Adapters principles, domain engines must remain pure and portable across web workers, edge runtimes, browser sandboxes, and containerized CI runners without inheriting host filesystem assumptions.
