---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - api-design
---
Asynchronous domain operations that evaluate, transform, or inspect collections of resources MUST yield results as an asynchronous stream or generator rather than buffering entire collections in memory.

Exception: Operations MAY return materialized collections IFF complete collection context is required for cross-item dependency resolution, holistic sorting, or global topological ordering.

Precedent: Reactive Streams; POSIX pipeline principles.

Rationale: Buffering collections in memory eliminates I/O and CPU concurrency, inflates memory overhead from O(1) to O(N), and prevents interactive consumers from processing results in real time.
