---
inspect:
  - diff
tags:
  - testing
---
Following *Software Engineering at Google* (Ch. 13, "Test Flakiness") and Martin Fowler (*Eradicating Non-Determinism in Tests*), tests waiting for asynchronous work, server readiness, or state transitions MUST synchronize on an observable signal (such as a channel, latch, barrier, or bounded condition poll) rather than a fixed `sleep()` delay.

Exception: A test mock or stub MAY call a bounded `sleep()` to simulate artificial upstream latency or timeout behavior IFF the test does not rely on that duration to order concurrent threads or await completion.
