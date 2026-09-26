---
paths:
  - "**/*.{sh,ts}"
inspect:
  - diff
---
Agent skill scripts MUST NOT emit superfluous visual formatting, ASCII art, decorative borders, progress spinners, or ANSI escape codes.

Rationale: Decorative terminal formatting and ANSI escape sequences consume precious LLM context tokens, confound regular expression parsers, and trigger security or terminal emulator errors in automated harnesses. Scripts designed for AI coding assistants must output clean, parseable text or structured key-value lines.

**Guidance:** Strip ANSI color escapes (`\x1b[...m`), spinners, and Unicode border characters. Ensure standard output consists strictly of unadorned, parseable plain text.
