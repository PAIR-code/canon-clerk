---
inspect:
  - diff
tags:
  - cli-ergonomics
---
When emitting plain text to non-interactive streams or pipelines, CLI list and query commands MUST default to unadorned, newline-separated records (one item per line), unless structured data or explicit formatting flags are requested (such as `--json`, `--table` and so on).

Rationale: In accordance with POSIX stream processing and clig.dev §Output, unadorned single-line records ensure list outputs compose directly with standard Unix utilities (`xargs`, `wc`, `grep`) without requiring custom regex post-processing.

**Guidance:** Suppress decorative headers, bullets, and table frames when outputting text to non-TTY pipes by default, preserving structured formats (`--json`) and explicit human layouts (`--table`) for opted-in invocations.
