# Tasks

## 1. TypeScript Types & AST Interfaces
- [ ] 1.1 Codify `InspectToken` and `InspectPlane` interfaces in `packages/schema/src/types/canon.ts`.
- [ ] 1.2 Update `CanonMetadata` with `readonly exists: readonly string[];` and `readonly inspect: readonly InspectPlane[];`.
- [ ] 1.3 Codify `CanonFrontmatter` interface in `packages/schema/src/types/canon.ts` with optional `exists` and `inspect` fields.
- [ ] 1.4 Purge legacy runner phase terminology ("Phase 1 Check", "Phase 3 Audit") from JSDoc comments in `packages/schema/src/types/`.
- [ ] 1.5 Export all updated types from `packages/schema/src/types/index.ts` and `packages/schema/src/index.ts`.

## 2. Metadata Derivation & Scope Containment
- [ ] 2.1 Implement `deriveExists` in `packages/schema/src/derive.ts` with scalar string coercion, default `[]`, and scoped path prefixing.
- [ ] 2.2 Update `deriveInspect` in `packages/schema/src/derive.ts` to parse `?` optional riders into `InspectPlane[]` with default `['diff', 'pr_title?', 'pr_body?']`.
- [ ] 2.3 Implement pure scope path validation in `packages/schema/src/derive.ts` rejecting `../` directory traversals for scoped canons.
- [ ] 2.4 Update `deriveMetadata` in `packages/schema/src/derive.ts` to assemble normalized `CanonMetadata` with `exists` and `inspect`.

## 3. Schema Parser Integration
- [ ] 3.1 Update `parseCanon` in `packages/schema/src/parse.ts` to return normalized `Canon` entities with `exists` and `inspect`.
- [ ] 3.2 Verify nominal fallback invariant derivation for 0-byte and heading-only canon files continues to function without errors.

## 4. AST Lint Rules & RuleContext
- [ ] 4.1 Add `scope` lazy getter to `RuleContext` in `packages/schema/src/context.ts`.
- [ ] 4.2 Update `no-unrecognized-keys` in `packages/schema/src/rules/frontmatter/no-unrecognized-keys.ts` to recognize `exists`.
- [ ] 4.3 Update `valid-frontmatter-types` in `packages/schema/src/rules/frontmatter/valid-frontmatter-types.ts` to validate `exists` types and `inspect` tokens with optional `?`.
- [ ] 4.4 Implement `scope-containment` rule in `packages/schema/src/rules/frontmatter/scope-containment.ts` to flag directory escapes in scoped canons.
- [ ] 4.5 Implement `exists-patterns` rule in `packages/schema/src/rules/frontmatter/exists-patterns.ts` to flag malformed glob syntax.
- [ ] 4.6 Register new rules in `FRONTMATTER_RULES` and `DEFAULT_RULES` catalogs.

## 5. Test Suite & Validation
- [ ] 5.1 Add unit tests in `derive.test.ts` for `exists` (omitted, scalar, array, scoped), `inspect` (riders, defaults, scalar), and scope escape rejection.
- [ ] 5.2 Add unit tests in `parse.test.ts` for full `Canon` entity parsing.
- [ ] 5.3 Add rule tests for `valid-frontmatter-types`, `scope-containment`, and `exists-patterns`.
- [ ] 5.4 Run all repository canons in `.canons/` through the updated schema runner to ensure 100% compliance.
- [ ] 5.5 Verify `npm run check` (typecheck, build, test, lint:specs) passes across the entire monorepo.
