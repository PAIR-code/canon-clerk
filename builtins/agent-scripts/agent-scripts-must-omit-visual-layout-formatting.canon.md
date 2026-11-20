---
triggers:
  - ".agents/skills/**"
tags:
  - agent-scripts
---
Agent skill scripts MUST NOT introduce decorative horizontal dividers, section banners, or space-padded columnar alignment (`column -t`). Output MUST remain unpadded delimiter-separated values, JSON, or unadorned key-value lines.

Rationale: Whitespace padding and punctuation divider rules consume excessive LLM context tokens on syntactic noise without improving model comprehension.
