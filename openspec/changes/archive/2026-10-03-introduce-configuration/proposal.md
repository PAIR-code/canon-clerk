# Proposal

## Why

During the research spike for Issue #175 (`feat(core): integrate Vercel AI SDK and Gemini provider for structured screening`), credential discovery and host environment resolution were found to exceed the architectural boundary of `@canon-clerk/core`.

`@canon-clerk/core` is the domain evaluation engine of Canon Clerk. Under `.canons/architecture/library-dependencies-must-be-explicit.md`, domain engines must declare all dependencies explicitly and operate strictly within the repository workspace envelope. Introducing host configuration libraries (`conf`, `env-paths`) and ambient OS filesystem access (`~/.config`, `%APPDATA%`, XDG) directly into `core` pollutes portable execution environments (such as web workers, edge runtimes, browser sandboxes, or containerized CI runners) with host OS filesystem assumptions.

Furthermore, conventional in-workspace `.env.local` files present significant secret exposure risks in modern AI pair-programming workflows. Autonomous and tool-using AI coding agents operate within the workspace envelope with broad file-reading capabilities; storing plaintext credentials inside the repository tree risks accidental exfiltration or LLM tool output leakage. Isolating credentials in standard user OS configuration directories (`~/.config/canon-clerk/config.json`) with `0o600` permissions (`-rw-------`) ensures secrets remain isolated from workspace-bound agents while naturally accessible to developer terminals and local processes.

To resolve these architectural concerns, this change establishes a dedicated adapter package, `@canon-clerk/configuration` (`packages/configuration`), providing workspace-piercing configuration discovery, environment cascading, and secure OS-level credential management.

## What Changes

1. **Pure Domain Contract in `@canon-clerk/core`:**
   - Define pure domain types (`ModelConfig`, `CascadeModelConfig`, `ModelTier`, `ParsedModelSpec`) and `parseModelSpec` in `packages/core/src/model-config.ts` without any external host configuration dependencies.

2. **Dedicated Adapter Package (`@canon-clerk/configuration`):**
   - Scaffold `packages/configuration` with ESM and declaration build configuration (`tsup.config.ts`), scoped Node types (`tsconfig.json`), and dynamic runtime version sourcing (`CONFIGURATION_VERSION`).
   - Implement `credentials.ts` for safe OS-level credential store management via `conf` and `env-paths`, enforcing POSIX `0o600` file permissions and non-throwing missing store diagnostics.
   - Implement `resolver.ts` providing deterministic 6-tier configuration cascade resolution (`resolveModelConfig`, `resolveCascadeModelConfig`), lone-key provider auto-inference, forward-compatible model defaults, and multi-key ambiguity detection with advisory warnings.

3. **Governing Canon Codification:**
   - Sanction and codify unstated repository invariants into `.canons/` covering workspace secret isolation, domain engine envelope boundaries, configuration specificity cascade, credential file permissions, and bundler runtime externalization.

4. **Living Documentation:**
   - Establish living specifications for the `cascade-configuration` capability and its nested sub-capabilities (`credentials`, `resolution`).

## Capabilities

### New Capabilities
- `cascade-configuration`: Pure evaluation cascade configuration contracts, model tier definitions, and canonical model specifier parsing.
- `cascade-configuration/credentials`: Secure OS-level credential storage and diagnostics outside the repository workspace envelope.
- `cascade-configuration/resolution`: Multi-source configuration resolution cascade with lone-key provider auto-inference and multi-key ambiguity warnings.

### Modified Capabilities
- None.

## Impact

- **New Packages:** `packages/configuration` (`@canon-clerk/configuration`).
- **Core Engine:** `packages/core` receives pure model configuration types.
- **Consumers:** Provides shared configuration resolution for `@canon-clerk/cli` and integration test suites.
- **Canons:** Adds foundational canons in `.canons/` under `security/`, `architecture/`, `configuration/`, and `build-systems/`.
- **Specs:** Establishes living specifications in `openspec/specs/cascade-configuration/`.
