---
triggers:
  - "README.md"
  - "SPEC.md"
  - "AGENTS.md"
references:
  - "SPEC.md"
  - "README.md"
  - "AGENTS.md"
inspect:
  - diff
tags:
  - internal
  - repo-governance
---
Canon definitions and property enumerations in `README.md` and `AGENTS.md` MUST strictly align with the normative heptad defined in `SPEC.md §1`.

Rationale: Divergent definitions across orientation documents and the specification create semantic ambiguity for human contributors and prompt drift for AI coding assistants.

**Remediation:** Synchronize the 7-attribute definition across all modified orientation files, ensuring `README.md` and `AGENTS.md` match `SPEC.md §1`.
