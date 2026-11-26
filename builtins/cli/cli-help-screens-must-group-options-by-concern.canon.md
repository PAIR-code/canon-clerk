---
inspect:
  - diff
tags:
  - cli
---
CLI `--help` screens exposing multiple distinct configuration concerns MUST organize flags into categorized sections rather than displaying a flat list.

Precedent: GNU Coding Standards §4.8; modern CLI help conventions (cargo, kubectl).

Rationale: Option grouping provides progressive disclosure, preventing advanced flags from obscuring primary workflow options.
