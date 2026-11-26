---
inspect:
  - diff
tags:
  - cli
---
CLI standard input consumption MUST require an explicit `-` positional operand rather than relying on stream TTY inspection or implicit pipe detection.

Precedent: POSIX.1-2017 Utility Syntax Guideline 10; Command Line Interface Guidelines (clig.dev §Pipes).

Rationale: Sniffing `isTTY` or pipe presence to read stdin implicitly causes commands to hang indefinitely in CI runners and subshells that allocate open, inactive standard input streams without EOF.
