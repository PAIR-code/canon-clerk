---
inspect:
  - diff
tags:
  - cli-ergonomics
---
CLI commands that report inspection results, audit verdicts, status summaries, or diagnostic lists MUST support a `--json` flag that outputs valid, unadorned JSON to `stdout`.

Rationale: CI automation and AI coding assistants require deterministic, structured data payloads rather than fragile regular expression scraping of human-oriented terminal text.

**Guidance:** Implement a `--json` flag that serializes command results directly via `JSON.stringify()` to `stdout`, ensuring all accompanying decorative formatting and status banners are omitted.
