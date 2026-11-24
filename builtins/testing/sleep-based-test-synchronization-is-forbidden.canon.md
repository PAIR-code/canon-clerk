---
inspect:
  - diff
tags:
  - testing
---
Tests waiting for asynchronous work, server readiness, or state transitions MUST synchronize on an observable signal (such as a channel, latch, barrier, or bounded condition poll) rather than a fixed `sleep()` delay.

Exception: A test mock or stub MAY call a bounded `sleep()` to simulate artificial upstream latency or timeout behavior IFF the test does not rely on that duration to order concurrent threads or await completion.

Rationale: Anchored in *Software Engineering at Google* (Ch. 13, "Test Flakiness") and Martin Fowler (*Eradicating Non-Determinism in Tests*), fixed sleep delays race against CPU scheduling on loaded CI runners, causing intermittent failures when too short and needless suite slowdowns when padded.
