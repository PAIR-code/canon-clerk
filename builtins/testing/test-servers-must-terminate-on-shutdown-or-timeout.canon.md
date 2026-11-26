---
inspect:
  - diff
tags:
  - testing
---
Test-spawned network servers and loopback listeners MUST terminate via an explicit shutdown signal rather than blocking for a fixed connection count.

Exception: A test server MAY omit an explicit shutdown signal IFF it bounds `accept()` with a finite timeout or the test deterministically executes exactly one unconditional client connection attempt against it prior to joining.

Precedent: *Software Engineering at Google* (Ch. 13, "Test Flakiness"); Gerard Meszaros (*xUnit Test Patterns*, "Erratic Test").

Rationale: Blocking accept loops convert connection-pooling reuse or early client failure into silent test-runner deadlocks instead of fast assertion diagnostics.
