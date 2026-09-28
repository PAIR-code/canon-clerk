---
governs:
  - "**/.canons/**/*.md"
inspect:
  - diff
tags:
  - internal
  - repo-governance
---
Canons not tagged `internal` MUST be project-agnostic and applicable to any project adopting that domain, free from repository-specific paths, tooling, or internal assumptions.

Rationale: Domain-scoped canons denote modular exemplars suitable for export packs and ecosystem guidance; contaminating portable domain canons with project-specific invariants misleads external adopters.

**Guidance:** If this canon enforces rules specific to this repository (such as internal directory structures, workspace conventions, or auxiliary tooling), add the `internal` tag. Otherwise, generalize the invariant so it holds for any arbitrary project adopting that domain.
