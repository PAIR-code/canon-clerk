---
inspect:
  - diff
tags:
  - cli
---
CLI flags configuring provider- or service-specific operational options MUST be namespace-qualified with the provider name rather than adopting generic root names.

Precedent: Multi-provider SDK conventions (OpenTelemetry, AWS CLI); Command Line Interface Guidelines (clig.dev §Arguments and flags).

Rationale: Generic names like `--api-key` presume a single vendor, forcing breaking deprecations when additional integrations arrive.

**Remediation:** Prefix provider-specific flags and environment variable fallbacks with the service slug (e.g. `--gemini-api-key` / `GEMINI_API_KEY`, `--github-token` / `GITHUB_TOKEN`), reserving generic flags strictly for common, vendor-agnostic abstractions.
