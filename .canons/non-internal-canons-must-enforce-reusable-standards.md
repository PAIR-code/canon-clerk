---
triggers:
  - "builtins/**/*.md"
inspect:
  - diff
tags:
  - internal
  - repo-governance
---
Built-in canons under `builtins/` (which lack the `internal` tag) MUST enforce reusable standards NOT limited to this specific project. They MUST be free from repository-specific paths, tooling, or internal assumptions.

Rationale: Built-in canons are compiled into the binary as portable `builtin:*` rules for community adoption; contaminating them with project-specifics precludes reuse.

**Remediation:** If this canon enforces rules specific to this repository (such as internal directory structures, workspace conventions, or auxiliary tooling), relocate it to `.canons/` with the `internal` tag. Otherwise, generalize the invariant so it holds for any project adopting that domain.
