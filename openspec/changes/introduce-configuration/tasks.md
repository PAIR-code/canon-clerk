# Tasks

## 1. Core Domain Port Contracts
- [x] 1.1 Establish `packages/core/src/model-config.ts` defining `ModelConfig`, `CascadeModelConfig`, `ModelTier`, `ParsedModelSpec`, and `parseModelSpec`.
- [x] 1.2 Export domain contracts from `packages/core/src/index.ts` and add unit tests in `packages/core/src/model-config.test.ts`.

## 2. Package Scaffolding
- [x] 2.1 Scaffold `packages/configuration/package.json` with workspace metadata, dependencies (`conf`, `env-paths`, `@canon-clerk/core`), and export fields.
- [x] 2.2 Configure `packages/configuration/tsconfig.json` with Node types and monorepo TypeScript references.
- [x] 2.3 Update root `tsup.config.ts` to build `packages/configuration` with ESM and declaration outputs, externalizing all runtime dependencies.
- [x] 2.4 Update root `vitest.config.ts` to include path alias for `@canon-clerk/configuration`.

## 3. OS Credential Store Implementation
- [x] 3.1 Implement `packages/configuration/src/credentials.ts` with `getCredentialFilePath`, `createCredentialStore` (`0o600`), `getStoredCredential`, `setStoredCredential`, `deleteStoredCredential`, and `inspectCredentialStore`.
- [x] 3.2 Add comprehensive hermetic tests in `packages/configuration/src/credentials.test.ts` covering path resolution, permission modes, CRUD operations, and non-throwing missing store diagnostics using isolated temporary directories.

## 4. Cascade Resolver & Auto-Inference
- [x] 4.1 Implement `packages/configuration/src/resolver.ts` with `resolveModelConfig` and `resolveCascadeModelConfig` following the 6-tier hierarchy.
- [x] 4.2 Implement lone-key provider auto-inference and forward-compatible default model cascading.
- [x] 4.3 Implement multi-key ambiguity detection with advisory warnings when competing provider credentials exist.
- [x] 4.4 Add exhaustive unit tests in `packages/configuration/src/resolver.test.ts` verifying all cascade tiers, provider auto-inference, and ambiguity warnings.

## 5. Barrel API Surface & Versioning
- [x] 5.1 Implement `packages/configuration/src/index.ts` exporting credential and resolver primitives, along with dynamic `CONFIGURATION_VERSION` sourced from `package.json`.
- [x] 5.2 Add barrel API surface tests in `packages/configuration/src/index.test.ts`.

## 6. Codify Sanctioned Governance Canons
- [x] 6.1 Author `.canons/security/credentials-must-reside-outside-workspace.md` forbidding in-workspace plaintext secrets.
- [x] 6.2 Author `.canons/architecture/workspace-piercing-by-domain-engines-is-forbidden.md` enforcing domain engine isolation.
- [x] 6.3 Author `.canons/configuration/configuration-resolution-must-follow-specificity-cascade.md` standardizing resolution precedence.
- [x] 6.4 Author `.canons/security/credential-stores-must-enforce-owner-only-permissions.md` requiring POSIX `0o600` file permissions.
- [x] 6.5 Author `.canons/build-systems/library-bundlers-must-externalize-runtime-dependencies.md` requiring externalization in Node library bundlers.
- [x] 6.6 Author `.canons/configuration/configuration-file-formats-must-conform-to-industry-standards.md` standardizing orthogonal provider and execution tier namespaces.

## 7. Living Specs & Monorepo Verification
- [x] 7.1 Sync OpenSpec delta specs into living documentation under `openspec/specs/cascade-configuration/`.
- [x] 7.2 Run full monorepo verification (`npm run check`) ensuring all lints, builds, and unit tests pass at 100%.
