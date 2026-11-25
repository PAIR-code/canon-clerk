---
triggers:
  - "{AGENTS,README,SPEC}.md"
references:
  - "{AGENTS,README,SPEC}.md"
inspect:
  - diff
tags:
  - internal
  - repo-governance
---
Canon definitions and property enumerations in `README.md` and `AGENTS.md` MUST strictly align with the normative heptad defined in `SPEC.md §1`.

Rationale: Divergent definitions across orientation documents and the specification create semantic ambiguity for human contributors and prompt drift for AI coding assistants.
