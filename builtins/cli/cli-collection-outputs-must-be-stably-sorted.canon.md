---
inspect:
  - diff
tags:
  - cli
---
Precedent: Reproducible Builds (reproducible-builds.org); POSIX collation rules.

Rationale: Deterministic sorting eliminates spurious snapshot diffs, flaky test assertions, and cognitive confusion in automated diff evaluation.

**Remediation:** Apply an explicit natural alphanumeric sort (e.g. sorting by file path, line number, or canonical identifier) to result collections prior to terminal rendering or JSON serialization.
