---
inspect:
  - diff
tags:
  - testing
---
Integration tests that assert POSIX termination signal traps (`SIGINT`, `SIGTERM`) on child processes MUST synchronize on process initialization before dispatching termination signals.

Precedent: POSIX.1-2017 (IEEE Std 1003.1); *Software Engineering at Google* (Ch. 13, "Test Flakiness").

Rationale: Child processes inherit default signal dispositions (`SIG_DFL`) until application signal handlers are registered; dispatching signals to an asynchronous process without an observable synchronization barrier causes race conditions where the kernel terminates the process prematurely.
