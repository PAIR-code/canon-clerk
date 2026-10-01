---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - api-design
  - asynchronous-programming
---
Asynchronous domain operations that evaluate, transform, or inspect collections of resources MUST yield results as an asynchronous stream or generator rather than buffering entire collections in memory.

Exception: Operations requiring complete collection context for cross-item dependency resolution, holistic sorting, or global topological ordering MAY return materialized collections.

Rationale: In accordance with Reactive Streams and POSIX pipeline principles, buffering collections in memory eliminates I/O and CPU concurrency, inflates memory overhead from O(1) to O(N), and prevents interactive consumers from processing results in real time.
