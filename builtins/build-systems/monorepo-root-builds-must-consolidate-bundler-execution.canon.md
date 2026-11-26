---
triggers:
  - "package.json"
  - "Makefile"
inspect:
  - diff
tags:
  - build-systems
  - monorepo
---
Monorepo root build scripts MUST consolidate compilation through a unified multi-entry bundler configuration rather than chaining serial workspace invocations.

Exception: Monorepo root builds MAY invoke per-package build targets IFF a dependency-graph task orchestrator executes independent packages concurrently.

Precedent: Peter Miller (*Recursive Make Considered Harmful*, 1997); *Software Engineering at Google* (Ch. 18, "Build Systems").

Rationale: Partitioning monorepo builds into recursive workspace sub-processes blinds the toolchain to the whole dependency graph, forfeiting worker-pool reuse and graph-level concurrency.
