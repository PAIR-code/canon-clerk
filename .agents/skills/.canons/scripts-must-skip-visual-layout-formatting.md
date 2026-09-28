---
triggers:
  - "**/*.{sh,ts}"
inspect:
  - diff
tags:
  - agent-scripts
---
Agent skill scripts MUST NOT introduce decorative horizontal dividers, section banners, or space-padded columnar alignment (`column -t`). Output MUST remain unpadded delimiter-separated values, JSON or unadorned key-value lines.

Rationale: Whitespace padding and punctuation divider rules consume excessive LLM context tokens on syntactic noise without improving model comprehension, corroding model confidence in the outputs and leading to tool re-runs.

**Guidance:** Do not pipe tabular streams into `column -t` or pad fields with spaces; emit raw tab-separated values (`\t`) or unadorned lines without horizontal divider rules.
