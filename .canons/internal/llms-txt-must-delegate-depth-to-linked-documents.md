---
paths:
  - "llms.txt"
references:
  - "llms.txt"
inspect:
  - diff
tags:
  - internal
  - repo-governance
---
The repository root `llms.txt` MUST only summarize and link to repository directories or files rather than inlining tutorial, explanatory, or specification content.

Rationale: Cold-read repository orientation requires minimal token overhead; inlining reference prose balloons agent prompt context and duplicates content that belongs in versioned documentation files.

**Guidance:** Relocate multi-paragraph explanations, in-depth guides, or technical specifications into dedicated Markdown documents under `docs/` or `specs/`, leaving only a single-line summary link in `llms.txt`.
