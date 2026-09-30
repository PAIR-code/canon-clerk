---
triggers:
  - "package.json"
  - "**/tsup.config.*"
inspect:
  - diff
tags:
  - build-systems
  - monorepo
  - performance
---
Monorepo root build scripts MUST NOT chain serial workspace invocations (such as sequential `npm run build -w` commands). Root builds MUST consolidate compilation through a unified multi-entry bundler configuration or orchestrate package builds concurrently along the dependency graph.

Rationale: In accordance with Peter Miller's *Recursive Make Considered Harmful* (1997) and *Software Engineering at Google* (Ch. 18, *Build Systems*), partitioning monorepo builds into recursive workspace sub-processes blinds the toolchain to the whole dependency graph, forfeiting worker-pool reuse and graph-level concurrency.
