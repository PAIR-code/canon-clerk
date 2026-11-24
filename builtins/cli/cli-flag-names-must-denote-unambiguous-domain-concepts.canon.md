---
inspect:
  - diff
tags:
  - cli
---
CLI long flag names MUST self-descriptively denote the target domain entity, input source, or behavioral effect at the invocation site rather than relying on cryptic abbreviations or generic nouns when multiple domain concepts could apply.

Exception: Well-established POSIX and GNU universal flag conventions (such as `--output`, `--format`, `--verbose`, `--quiet`, `--help`, or `--version`) MAY be used IFF their behavior matches standard Unix semantics.

Precedent: Command Line Interface Guidelines (clig.dev); POSIX utility syntax conventions.
