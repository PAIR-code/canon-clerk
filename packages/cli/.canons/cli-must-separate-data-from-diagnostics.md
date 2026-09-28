---
inspect:
  - diff
tags:
  - cli-ergonomics
---
CLI commands MUST emit primary payload data exclusively to `stdout`, and MUST direct all diagnostic logging, progress indicators, warnings, and error messages to `stderr`.

Rationale: Downstream Unix pipelines consume `stdout` as uncorrupted data streams; interleaving diagnostic messages into `stdout` breaks machine parsers and piped utilities.

**Guidance:** Redirect informational logs, banners, and progress spinners to `stderr` (e.g. via `console.error` or `process.stderr.write`), reserving `stdout` strictly for user-requested command output or data payloads.
