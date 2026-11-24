---
inspect:
  - diff
tags:
  - cli
---
Per Command Line Interface Guidelines (clig.dev) and POSIX utility syntax conventions, CLI long flag names MUST unambiguously express the domain entity, input source, or behavioral effect on a cold read rather than relying on cryptic abbreviations or generic nouns when multiple domain concepts could apply.

Exception: Well-established POSIX and GNU universal flag conventions (such as `--output`, `--format`, `--verbose`, `--quiet`, `--help`, or `--version`) MAY be used IFF their behavior matches standard Unix semantics.
