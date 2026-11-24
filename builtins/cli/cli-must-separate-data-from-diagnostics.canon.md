---
inspect:
  - diff
tags:
  - cli
---
CLI commands MUST direct operational diagnostic logging, progress indicators, spinners, warnings, and error messages to `stderr`, reserving `stdout` exclusively for requested command output.

Exception: Multi-format CLI commands MAY render human-readable summary reports and provenance headers to `stdout` IFF the caller selected a human-facing display format while machine-consumable modes (such as `--json` or delimiter-separated output) emit pure payload data on `stdout`.

Rationale: Per the Unix Rule of Composition (McIlroy) and Command Line Interface Guidelines (clig.dev), downstream pipelines consume `stdout` as uncorrupted output; interleaving operational telemetry or warnings into `stdout` breaks machine parsers.
