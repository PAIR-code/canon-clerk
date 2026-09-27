---
paths:
  - ".agents/skills/**/SKILL.md"
inspect:
  - diff
  - pr_body
---
`SKILL.md` body markdown MUST NOT duplicate trigger conditions or prompt keywords declared in frontmatter `description:`.

Rationale: Skill activation is driven by frontmatter metadata; repeating triggers in the body consumes tokens without aiding runtime execution.
