---
inspect:
  - diff
tags:
  - cli
---
Boolean CLI flags MUST be declared with positive polarity, providing a `--no-<flag>` prefix for negation rather than introducing inherently negative flag names.

Precedent: Command Line Interface Guidelines (clig.dev §Arguments and flags); GNU option conventions.

Rationale: Inherently negative flags create confusing double-negatives when passed boolean arguments in scripts or configuration files (e.g. `--no-validation=false`).
