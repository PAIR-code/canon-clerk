# Tasks

## 1. Stage 0 Scope Containment Primitive
- [ ] 1.1 Implement `checkFileInCanonScope` in `packages/core/src/scope.ts` along with `ScopeMismatchReason`, `ScopeCheckResult`, and `CheckFileInCanonScopeOptions` types.
- [ ] 1.2 Implement comprehensive unit tests in `packages/core/src/scope.test.ts` structured into topical sub-suites covering workspace boundary verification, ignore patterns, glob discovery validation, hierarchical ancestor walks, and path normalization.

## 2. Stage 0 Trigger Matching Primitive
- [ ] 2.1 Implement `matchesTriggers` in `packages/core/src/triggers.ts` along with `TriggerCheckResult` type and dotfile-aware glob evaluation.
- [ ] 2.2 Implement comprehensive unit tests in `packages/core/src/triggers.test.ts` structured into topical sub-suites covering omitted/empty triggers, universal wildcards, path patterns, extension globs, dotfiles, and non-matching paths.

## 3. Package Integration & Verification
- [ ] 3.1 Export `checkFileInCanonScope`, `matchesTriggers`, and associated types from `packages/core/src/index.ts`.
- [ ] 3.2 Run full repository verification suite (`npm run check`) and ensure all build, test, typecheck, lint, and canon dogfood checks pass cleanly.
