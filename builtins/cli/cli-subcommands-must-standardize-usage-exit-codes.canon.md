---
inspect:
  - diff
tags:
  - cli
---
Hierarchical CLI subcommands MUST terminate with exit status `2` on option, argument, and syntax errors across all command nesting levels.

Exception: Declarative argument-parsing frameworks whose root parser centrally enforces exit status `2` across nested subcommands MAY omit per-subcommand exit-override hooks IFF syntax and usage errors uniformly exit with status `2` at runtime.

Precedent: GNU Coding Standards §4.3; BSD `sysexits.h`.

Rationale: Exit status `2` distinguishes invocation and syntax defects from domain or runtime failures (`exit 1`); imperative CLI frameworks whose subcommands default to `exit 1` require explicit parser exit overrides.
