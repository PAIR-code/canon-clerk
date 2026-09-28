---
governs:
  - "**/.canons/**/*.md"
inspect:
  - diff
tags:
  - internal
  - repo-governance
---
Canons tagged `internal` MUST enforce invariants that narrowly apply to this repository due to local constraints, workspace layout, or project-specific desires, and MUST NOT be applied to reusable, domain-specific standards.

Rationale: The `internal` tag isolates rules unique to this project's governance; mislabeling modular domain guidance as `internal` creates a false binary, obscuring portable domain packs.

**Guidance:** If this canon expresses general best practices for a specific domain, technology, or subsystem (such as agent skills, agent scripts, or CLI ergonomics) rather than constraints unique to Canon Clerk, remove the `internal` tag and retain only its domain tags. Reserve `internal` strictly for rules governing this repository's local architecture, directory structure, or project-specific policies.
