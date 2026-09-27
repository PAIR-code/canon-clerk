---
paths:
  - "**/.canons/**/*.md"
inspect:
  - diff
tags:
  - internal
  - repo-governance
---
Canons tagged `universal` MUST be project-agnostic and universally applicable to any project adopting the canon specification, free from repository-specific paths, tooling, or internal assumptions.

Rationale: The `universal` tag denotes canonical exemplars suitable for export packs and ecosystem guidance; contaminating universal canons with project-specific invariants misleads external adopters.

**Guidance:** If this canon enforces rules specific to this repository (such as internal directory structures, workspace conventions, or auxiliary tooling), replace the `universal` tag with `internal`. If it enforces modular, reusable practices for an optional domain (such as agent skills, agent scripts, or CLI ergonomics), remove `universal` and retain only domain-specific tags. Otherwise, generalize the invariant so it holds for any arbitrary project adopting canons.
