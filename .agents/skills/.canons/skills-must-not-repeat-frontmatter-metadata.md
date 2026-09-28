---
triggers:
  - ".agents/skills/**/SKILL.md"
inspect:
  - diff
  - pr_body
tags:
  - agent-skills
---
`SKILL.md` body markdown MUST NOT duplicate trigger conditions or prompt keywords declared in frontmatter `description:`.

Rationale: Skill activation is driven by frontmatter metadata; repeating triggers in the body consumes tokens without aiding runtime execution.

**Guidance:** Remove redundant activation prompts, keyword triggers, or slash command invocations from the body markdown, retaining them exclusively in frontmatter `description:`.
