---
inspect:
  - diff
tags:
  - cli
---
CLI processes MUST register handlers for termination signals (`SIGINT`, `SIGTERM`) that clean up active temporary resources and terminate with standard signal exit codes rather than emitting unhandled exception traces.

Precedent: POSIX Signal Handling (IEEE Std 1003.1 §2.4); standard `128 + N` exit conventions.

Rationale: Signal traps prevent orphaned scratch locks and suppress raw unhandled promise rejections.

**Remediation:** Trap `SIGINT` and `SIGTERM` to invoke cleanup hooks that unlink temporary files and exit immediately with status `130` (`128 + SIGINT`) or `143` (`128 + SIGTERM`).
