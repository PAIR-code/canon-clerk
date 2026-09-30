---
triggers:
  - ".agents/skills/**/SKILL.md"
tags:
  - agent-skills
---
`SKILL.md` bodies MUST NOT duplicate activation triggers or prompt keywords declared in frontmatter `description:`.

Rationale: Skill activation is driven by frontmatter metadata; duplicating triggers in the body wastes assistant context without aiding runtime execution.
