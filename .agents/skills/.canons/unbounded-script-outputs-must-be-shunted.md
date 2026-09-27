---
paths:
  - "**/*.{sh,ts}"
inspect:
  - diff
tags:
  - agent-scripts
---
Agent skill scripts that invoke commands with potentially unbounded or high-volume output MUST redirect output to a temporary file when exceeding 8KB.

Rationale: AI coding assistant harnesses enforce strict stdout caps (typically 8KB to 32KB). When scripts emit unbounded command outputs directly to standard output, critical error messages or diagnostic trails are truncated. Capturing high-volume output and writing it to a temporary file prevents truncation and enables assistants to inspect logs using paginated tools (`view_file`) or targeted search (`grep`).

**Guidance:** When executing commands that may produce extensive output (such as full git logs, test suite runs, or raw API payloads), buffer or measure output volume. If output exceeds 8KB, redirect the complete stream to a temporary file (e.g. in `/tmp/` or a designated scratch path) and print the resulting file path alongside a compact summary or failure slice.
