---
triggers:
  - ".agents/skills/**/SKILL.md"
inspect:
  - diff
  - pr_body
tags:
  - internal
  - agent-skills
---
Agent skills that wrap the authoring, templating, or editing of declarative configuration files are forbidden. AI assistants MUST inspect and modify declarative files directly in the workspace.

Rationale: Declarative configuration files (such as YAML, JSON, or TOML files tracked in Git) are self-describing; coding assistants can inspect and modify them directly in the workspace without specialized skill layers.
