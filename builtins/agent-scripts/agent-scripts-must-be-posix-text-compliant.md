---
triggers:
  - ".agents/skills/**"
tags:
  - agent-scripts
---
Agent skill scripts MUST emit standard POSIX text streams, consisting exclusively of printable characters and standard newline terminators (`\n`), free from ANSI terminal escapes (`\x1b[...m`), carriage returns (`\r`), or progress spinners.

Rationale: Terminal escape sequences, animated spinners, and carriage returns consume model tokens, corrupt automated parsers, and crash non-interactive subshell harnesses.
