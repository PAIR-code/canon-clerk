---
paths:
  - "**/AGENTS.md"
inspect:
  - diff
tags:
  - agent-orientation
---
`AGENTS.md` content MUST contain ONLY instructions and orientation that apply UNCONDITIONALLY to every AI agent session in the repository or workspace. Instructions that are only conditionally useful (task-specific playbooks, script usage guides, or situational commands) MUST NOT appear in `AGENTS.md`.

Rationale: `AGENTS.md` is ingested unconditionally into every AI assistant interaction; inlining situational instructions dilutes prompt attention and duplicates on-demand skills.

**Guidance:** Keep `AGENTS.md` strictly focused on global workspace orientation, layout constraints, and universal behavioral rules. If adding step-by-step procedures, tool arguments, or situational workflows, encapsulate them in dedicated on-demand skills or referenced documentation.
