---
id: canon-0001-falsifiable-canons
title: Canons Must Be Specific, Atomic, and Falsifiable
paths:
  - "**/.canons/**/*.md"
inspect:
  - diff
---

## Rule
Every canon must codify a concrete, falsifiable invariant. It must define unambiguous evaluation criteria, cover only a single cohesive concern, and provide explicit escape hatches.

## Rationale
Vague, composite, or purely stylistic canons (e.g., "Write clean, readable code") cause non-deterministic CI audits, hallucinated failures, and developer fatigue.

## Evaluation Criteria
- **Inapplicable**: Modifications to non-canon files or documentation outside `.canons/`.
- **Pass**:
  - Contains valid YAML frontmatter with required fields (`id`, `title`).
  - Contains discrete `## Rule` and `## Rationale` sections.
  - Defines explicit `## Evaluation Criteria` with at least `Inapplicable`, `Pass`, and `Fail` scenarios.
  - The rule addresses an invariant that traditional AST linters (ESLint, Prettier, compilers) cannot enforce.
  - Focuses on a single, atomic policy rather than combining multiple disparate requirements.
- **Fail**:
  - The canon is subjective, advisory, or aspirational (e.g., "Prefer simple solutions").
  - The canon combines multiple unrelated rules into one file.
  - The canon duplicates existing static analysis (e.g., banning tabs or enforcing file naming conventions already handled by linters).
  - The canon lacks an `Inapplicable` escape hatch.
  - The frontmatter fails basic schema validation.

