---
inspect:
  - diff
tags:
  - cli-ergonomics
---
CLI flags configuring persistent operational options (such as API keys, model selectors, and log verbosity) MUST support configuration via environment variables when the command-line flag is omitted.

Rationale: Containerized environments, secret managers, and CI systems inject credentials and runtime parameters via environment variables; requiring explicit CLI flags risks exposing secrets in process tables and shell history.

**Guidance:** Register environment variable fallbacks (e.g. `GEMINI_API_KEY` for `--gemini-api-key`) in the CLI option definition, ensuring flag values take precedence when explicitly supplied.
