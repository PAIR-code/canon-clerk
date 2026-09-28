---
governs:
  - "**/*.{sh,ts}"
inspect:
  - diff
tags:
  - agent-scripts
---
Agent skill scripts MUST emit standard POSIX text streams, consisting exclusively of printable characters and standard newline terminators (`\n`), free from ANSI terminal escapes (`\x1b[...m`), carriage returns (`\r`), or progress spinners.

Rationale: Terminal escape sequences, animated spinners, and carriage returns consume precious model tokens, corrupt automated parsers, and crash non-interactive subshell harnesses.

**Guidance:** Strip ANSI color escapes (e.g. `sed 's/\x1b\[[0-9;]*[a-zA-Z]//g'`), eliminate carriage returns (`\r`), and ensure all output lines terminate with standard newlines (`\n`).
