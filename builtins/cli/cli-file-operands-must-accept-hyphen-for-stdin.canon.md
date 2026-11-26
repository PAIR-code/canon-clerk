---
inspect:
  - diff
tags:
  - cli
---
When introducing or modifying a CLI file-input operand that reads an input file from disk, the operand MUST accept `-` to read from standard input (`stdin`) in place of a filesystem path.

Precedent: POSIX.1-2017 Utility Syntax Guideline 10.

Rationale: Recognizing `-` as standard input on file-input operands enables commands to compose cleanly into Unix pipelines without requiring intermediate temporary files.
