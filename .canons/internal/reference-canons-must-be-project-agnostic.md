---
paths:
  - "**/.canons/**/*.md"
inspect:
  - diff
tags:
  - internal
  - repo-governance
---
Canons tagged `reference` MUST be project-agnostic and universally applicable to any project adopting the canon specification, free from repository-specific paths, tooling, or internal assumptions.

Rationale: The `reference` tag denotes canonical exemplars suitable for export packs and ecosystem guidance; contaminating reference canons with project-specific invariants misleads external adopters.

**Guidance:** If this canon enforces rules specific to this repository (such as internal directory structures, workspace conventions, or auxiliary tooling), replace the `reference` tag with `internal`. Otherwise, generalize the invariant so it holds for any arbitrary project adopting canons.
