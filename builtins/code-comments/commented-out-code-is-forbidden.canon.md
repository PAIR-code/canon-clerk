---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
Source files MUST NOT retain commented-out code blocks.

Exception: Commented code snippets MAY appear IFF they are illustrative usage examples embedded inside documentation comments.

Rationale: Per *Clean Code* (Martin) and Google Style Guides, dead code preserved in comments rots out of sync with the surrounding codebase and obscures active logic while version control already retains historical implementations.
