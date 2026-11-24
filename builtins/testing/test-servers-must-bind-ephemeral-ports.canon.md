---
inspect:
  - diff
tags:
  - testing
---
Test-spawned network servers and loopback listeners (such as mock HTTP or TCP servers) MUST bind an OS-assigned ephemeral port (`:0`) rather than a hardcoded port number.

Rationale: Anchored in Meszaros (*xUnit Test Patterns*, "Shared Fixture") and *Software Engineering at Google* (Ch. 13, "Test Flakiness"), hardcoded loopback ports cause `EADDRINUSE` collisions across parallel test threads and concurrent CI jobs.
