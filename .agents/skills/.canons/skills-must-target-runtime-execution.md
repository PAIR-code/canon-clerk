---
paths:
  - ".agents/skills/**/SKILL.md"
inspect:
  - diff
  - pr_body
---
`SKILL.md` MUST be authored strictly as an operational runbook for the executing AI assistant, omitting background narratives, design rationale, and script authoring standards.

Rationale: Skills load directly into the assistant's active context during execution; narrative background dilutes attention and wastes tokens.

**Guidance:** Move architectural background or design rationale to `docs/` or PR descriptions.
