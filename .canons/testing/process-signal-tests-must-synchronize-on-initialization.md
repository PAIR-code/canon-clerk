---
inspect:
  - diff
tags:
  - testing
---
Integration tests that assert POSIX termination signal traps (`SIGINT`, `SIGTERM`) on child processes MUST synchronize on process initialization before dispatching termination signals.

Rationale: Under POSIX.1-2017 (IEEE Std 1003.1), child processes inherit default signal dispositions (`SIG_DFL`) until application signal handlers are registered. As noted in *Software Engineering at Google* (Ch. 13, "Test Flakiness"), dispatching signals to an asynchronous process without an observable synchronization barrier causes race conditions where the kernel terminates the process prematurely.
