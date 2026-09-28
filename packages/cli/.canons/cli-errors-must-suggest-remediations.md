---
inspect:
  - diff
tags:
  - cli-ergonomics
---
CLI error messages for user-correctable errors MUST provide actionable remediation instructions—including the exact command, flag, or environment variable required to resolve the defect—rather than printing raw stack traces.

Rationale: Stack traces expose internal implementation details without explaining corrective action, increasing developer debugging latency and frustrating users who encounter expected operational errors.

**Guidance:** Catch known domain and configuration exceptions, presenting a concise description of the failure alongside a copy-pasteable remediation command or configuration example.
