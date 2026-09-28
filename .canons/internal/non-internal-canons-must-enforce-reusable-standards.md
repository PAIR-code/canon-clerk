---
governs:
  - "**/.canons/**/*.md"
inspect:
  - diff
tags:
  - internal
  - repo-governance
---
Canons that lack the `internal` tag MUST enforce reusable standards NOT limited to this specific project. They MUST be free from repository-specific paths, tooling, or internal assumptions.

Rationale: Non-`internal` canons are intended to be modular, portable exemplars for export packs and community adoption; contaminating them with project-specifics precludes reuse.

**Guidance:** If this canon enforces rules specific to this repository (such as internal directory structures, workspace conventions, or auxiliary tooling), add the `internal` tag. Otherwise, generalize the invariant so it holds for any project adopting that domain.
