---
inspect:
  - diff
tags:
  - cli-ergonomics
---
CLI query, inspection, and diagnostic commands MUST remain strictly read-only, preserving the working tree without mutating files, touching timestamps, or creating untracked cache directories.

Rationale: Citing POSIX query idempotency and linter purity standards, inspection commands must never dirty working trees or invalidate build caches.
