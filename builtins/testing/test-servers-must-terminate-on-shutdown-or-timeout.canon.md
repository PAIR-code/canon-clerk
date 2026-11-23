---
inspect:
  - diff
tags:
  - testing
---
Test-spawned network servers and loopback listeners (such as mock HTTP or TCP servers) MUST terminate via an explicit shutdown signal or bounded `accept()` timeout rather than blocking for a fixed connection count.

Exception: A test server MAY block on a single `accept()` call without a shutdown signal IFF the test deterministically executes exactly one unconditional client connection attempt against it prior to joining.

Rationale: Anchored in *Software Engineering at Google* (Ch. 13, "Test Flakiness") and Meszaros (*xUnit Test Patterns*, "Erratic Test"), blocking accept loops convert connection-pooling reuse or early client failure into silent test-runner deadlocks instead of fast assertion diagnostics.
