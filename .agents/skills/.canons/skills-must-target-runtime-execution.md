---
paths:
  - ".agents/skills/**/SKILL.md"
inspect:
  - diff
  - pr_body
---
`SKILL.md` documentation MUST be authored strictly as an operational runbook for the executing AI assistant. `SKILL.md` MUST NOT contain historical problem narratives, PR justifications, design rationale, or authoring standards that the executing agent does not need at runtime.

Rationale: Skills are ingested directly into an AI assistant's active context window at the exact moment of execution. Background narratives, motivation essays, and script authoring guidelines dilute attention on the operational task and waste context tokens on dead weight. Motivation belongs in commit and PR descriptions; persistent background documentation belongs in `docs/`.

**Guidance:** Keep `SKILL.md` focused strictly on executable invocation syntax, flags, and operational invariants. Omit "Motivation", "Architectural Model", "Problem Statement", and "Script Design Standards" sections.
