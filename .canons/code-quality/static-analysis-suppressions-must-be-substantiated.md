---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-quality
  - security
---
Inline static-analysis, linter, and security-scanner suppression directives (such as `zizmor: ignore`, `#[allow(...)]`, `eslint-disable`, `noqa`, `shellcheck disable`, or `@ts-ignore`) MUST include a specific justification that is factually substantiated by the surrounding code.

Rationale: Per OWASP Code Review and static-analysis governance practices, deterministic linters can verify that a suppression comment exists but cannot verify whether its stated claim holds in the surrounding code; vacuous or inaccurate suppressions silently disable safety and security gates.
