---
triggers:
  - "**/src/**"
  - "**/*.{rs,py,ts,js,go,java,kt}"
inspect:
  - diff
tags:
  - concurrency
---
Pooled or multiplexed asynchronous I/O clients shared across threads or synchronous blocking boundaries MUST execute on a persistent, continuously driven runtime whose lifetime encompasses the client pool, rather than on per-call or thread-local single-threaded executors.

Exception: An asynchronous I/O client MAY execute on an ephemeral per-call or single-threaded executor IFF the client instance is constructed, consumed, and dropped strictly within a single synchronous blocking invocation without sharing or pooling handles across calls or threads.

Precedent: HTTP/1.1 persistent connections (RFC 9112); HTTP/2 stream multiplexing (RFC 9113).

Rationale: Under cooperative async reactor models, pooled clients register sockets and spawn background connection-driver tasks onto the executor active during handshake; parking or dropping a single-threaded executor between blocking calls or upon worker-thread exit starves concurrent and subsequent requests reusing pooled connections until timeout.
