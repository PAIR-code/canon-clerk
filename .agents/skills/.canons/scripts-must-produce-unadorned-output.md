---
paths:
  - "**/*.{sh,ts}"
inspect:
  - diff
tags:
  - agent-scripts
---
Agent skill scripts MUST NOT emit superfluous visual formatting, ASCII art, decorative borders, progress spinners, or ANSI escape codes.

Rationale: Terminal escapes, visual frames, and interactive spinners consume precious model context tokens and corrupt automated parsers with raw escape sequences.

**Guidance:** Strip ANSI color escapes (`\x1b[...m`), spinners, and Unicode border characters. Ensure standard output consists strictly of unadorned, parseable plain text.
