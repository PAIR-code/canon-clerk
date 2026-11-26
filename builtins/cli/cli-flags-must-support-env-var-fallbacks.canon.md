---
inspect:
  - diff
tags:
  - cli
---
CLI flags configuring persistent operational options MUST support configuration via environment variables when the command-line flag is omitted.

Precedent: The Twelve-Factor App (§III, Config); Command Line Interface Guidelines (clig.dev §Environment variables).

Rationale: Environment variable fallbacks prevent credential leaks in process tables and support seamless container and secret-manager injection.

**Remediation:** Register environment variable fallbacks (e.g. `GEMINI_API_KEY` for `--gemini-api-key`) in the CLI option definition, ensuring flag values take precedence when explicitly supplied.
