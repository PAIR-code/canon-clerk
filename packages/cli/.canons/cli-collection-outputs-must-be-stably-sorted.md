---
inspect:
  - diff
tags:
  - cli-ergonomics
---
CLI commands that report lists or collections of items (such as canons, diagnostic findings, or files) MUST output them in a deterministic, stable sort order.

Rationale: Nondeterministic ordering across runs breaks reproducible build assertions, generates noisy diffs in snapshot tests, and confounds AI agent comparison analysis.

**Guidance:** Apply an explicit natural alphanumeric sort (e.g. sorting by file path, line number, or canonical identifier) to result collections prior to terminal rendering or JSON serialization.
