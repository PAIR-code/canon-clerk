---
inspect:
  - diff
tags:
  - testing
---
Following Meszaros (*xUnit Test Patterns*, "Shared Fixture") and *Software Engineering at Google* (Ch. 13, "Test Flakiness"), test-spawned network servers and loopback listeners (such as mock HTTP or TCP servers) MUST bind an OS-assigned ephemeral port (`:0`) rather than a hardcoded port number.
