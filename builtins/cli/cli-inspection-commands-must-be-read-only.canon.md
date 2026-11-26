---
inspect:
  - diff
tags:
  - cli
---
CLI query, inspection, and diagnostic commands MUST remain strictly read-only, preserving the working tree without mutating files, touching timestamps, or creating untracked cache directories.

Precedent: POSIX query idempotency; static-analysis linter purity standards.

Rationale: Mutating state during read-only inspection dirties working trees and invalidates build caches.
