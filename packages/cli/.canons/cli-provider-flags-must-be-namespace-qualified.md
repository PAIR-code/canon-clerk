---
inspect:
  - diff
tags:
  - cli-ergonomics
---
CLI flags configuring provider- or service-specific operational options (such as credentials, endpoints, or vendor settings) MUST be namespace-qualified with the provider name (e.g. `--<provider>-<option>`) rather than adopting generic root names.

Rationale: Generic flags like `--api-key` or `--endpoint` presume a single provider; introducing subsequent integrations forces breaking CLI deprecations or complex, error-prone mode-dependent flag semantics.

**Guidance:** Prefix provider-specific flags and environment variable fallbacks with the service slug (e.g. `--gemini-api-key` / `GEMINI_API_KEY`, `--github-token` / `GITHUB_TOKEN`), reserving generic flags strictly for universal, vendor-agnostic abstractions.
