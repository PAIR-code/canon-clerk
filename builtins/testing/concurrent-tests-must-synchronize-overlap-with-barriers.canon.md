---
inspect:
  - diff
tags:
  - testing
---
Tests asserting simultaneous execution or resource contention (such as concurrent connections, parallel in-flight requests, or lock contention) MUST enforce overlap via an explicit synchronization barrier or latch rather than relying on thread-scheduling timing.

Rationale: Anchored in *Software Engineering at Google* (Ch. 13, "Test Flakiness"), spawning parallel tasks without an observable rendezvous allows fast workers to complete before sibling tasks start on loaded or low-core CI runners, causing intermittent pool reuse or missed-contention flakes.
