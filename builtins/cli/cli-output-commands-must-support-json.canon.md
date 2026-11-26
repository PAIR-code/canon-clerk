---
inspect:
  - diff
tags:
  - cli
---
CLI commands that report inspection results, audit verdicts, status summaries, or diagnostic lists MUST support a `--json` flag that outputs valid, unadorned JSON to `stdout`.

Precedent: Command Line Interface Guidelines (clig.dev §Output).

Rationale: Automated CI systems and AI agents require deterministic machine-readable JSON payloads rather than fragile scraping of human-oriented terminal text.
