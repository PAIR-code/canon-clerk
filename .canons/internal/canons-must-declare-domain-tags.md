---
triggers:
  - "**/.canons/**/*.md"
inspect:
  - diff
tags:
  - internal
  - repo-governance
---
Each canon file MUST declare a `tags` frontmatter field specifying one or more domain tags that accurately categorize the canon for export packaging and selective adoption.

Rationale: Automated tooling in Canon Clerk relies on domain tags to assemble modular rule packs and presets (such as `canon-authoring`, `cli-ergonomics`, and `agent-skills`); omitting domain tags renders canons undiscoverable for packaging.

**Guidance:** Add a `tags:` list to the frontmatter classifying the application domain or subsystem governed by this canon (e.g. `canon-authoring`, `cli-ergonomics`, `agent-skills`, `conventional-commits`). If the canon enforces rules private to this repository's local conventions, include the `internal` tag.
