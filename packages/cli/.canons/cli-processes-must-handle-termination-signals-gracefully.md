---
inspect:
  - diff
tags:
  - cli-ergonomics
---
CLI processes MUST register handlers for termination signals (`SIGINT`, `SIGTERM`) that clean up active temporary resources and terminate with standard signal exit codes rather than emitting unhandled exception traces.

Rationale: Abrupt termination without signal traps leaves orphaned scratch files and locks on disk, while spewing raw unhandled promise rejections into interactive terminal sessions.

**Guidance:** Trap `SIGINT` and `SIGTERM` to invoke cleanup hooks that unlink temporary files and exit immediately with status `130` (`128 + SIGINT`) or `143` (`128 + SIGTERM`).
