---
inspect:
  - diff
tags:
  - cli
---
Primary operational targets of a CLI command MUST be accepted as positional arguments rather than requiring named flags.

Precedent: POSIX Utility Syntax Guidelines 4 & 5; Command Line Interface Guidelines (clig.dev, "Arguments for what, flags for how").

Rationale: Positional operands optimize shell expansion and eliminate redundant flag boilerplate.
