---
inspect:
  - diff
tags:
  - testing
---
Tests asserting simultaneous execution or resource contention MUST enforce overlap via an explicit synchronization barrier or latch rather than relying on thread-scheduling timing.

Precedent: *Software Engineering at Google* (Ch. 13, "Test Flakiness").

Rationale: Spawning parallel tasks without an observable rendezvous allows fast workers to complete before sibling tasks start on loaded or low-core CI runners, causing intermittent pool reuse or missed-contention flakes.
