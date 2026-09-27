---
paths:
  - ".agents/skills/**/SKILL.md"
inspect:
  - diff
  - pr_body
---
`SKILL.md` body markdown MUST NOT repeat trigger condition lists or keyword enumerations already declared in the YAML frontmatter `description:`.

Rationale: The agent harness uses the YAML frontmatter `description:` to discover and activate the skill. Once the skill is activated and read into context, the agent is already in the execution phase. A markdown section listing prompt triggers is redundant, consumes unnecessary tokens, and creates maintenance overhead when trigger phrases evolve.

**Guidance:** Eliminate markdown sections like "Trigger Conditions & Discovery". Rely exclusively on the YAML frontmatter `description:` for keyword discovery and intent matching.
