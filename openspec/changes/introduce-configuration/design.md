# Design

## Context

Canon Clerk's Three-Phase Evaluation Cascade evaluates repository changes across Phase 1: Check (deterministic linting and trigger matching), Phase 2: Docket (relevance and jurisdiction screening), and Phase 3: Audit (substantive adjudication). Phases 2 and 3 require access to generative language models, requiring model configuration (providers, model identifiers, API keys, and endpoint URLs).

During initial prototyping, configuration discovery was initially colocated with model execution within `@canon-clerk/core`. However, resolving host environment variables, locating operating system configuration directories (`~/.config`, `%APPDATA%`, XDG), and managing persisted credentials fundamentally exceed the domain scope of `core`. Per `.canons/architecture/library-dependencies-must-be-explicit.md`, domain engines must be pure and portable, operating strictly within the repository workspace envelope.

Simultaneously, conventional in-workspace `.env.local` patterns present major security concerns in AI pair-programming environments. Autonomous and tool-using AI coding agents operate within the workspace with arbitrary file-reading capabilities; plaintext credentials stored in the workspace risk unintentional exfiltration into model contexts or tool outputs.

## Goals / Non-Goals

**Goals:**
- Decouple workspace-piercing configuration discovery and OS-level credential management into a dedicated adapter package (`@canon-clerk/configuration`).
- Maintain the purity and portability of `@canon-clerk/core`, restricting its responsibility to pure domain evaluation and typed configuration ports.
- Enforce strict OS-level credential isolation outside the workspace envelope with POSIX `0o600` permissions.
- Implement a deterministic 6-tier configuration resolution cascade supporting lone-key provider auto-inference and multi-key ambiguity detection.
- Provide a single source of truth for configuration resolution across CLI subcommands and integration testing harnesses.
- Codify foundational governance invariants into `.canons/`.

**Non-Goals:**
- CLI flag parsing (handled downstream by `@canon-clerk/cli`).
- AI model execution or HTTP transport (handled by `@canon-clerk/core`).
- Online network calls during unit testing (all package tests remain hermetic).

## Decisions

### 1. Dedicated Adapter Package vs In-Engine Resolution
- **Decision:** Extract configuration resolution and credential storage into `@canon-clerk/configuration` rather than housing them inside `@canon-clerk/core`.
- **Rationale:** Keeps `core` portable across browser sandboxes, web workers, and containerized CI runners without pulling in Node.js host filesystem dependencies. `core` defines the typed contract port (`ModelConfig` / `CascadeModelConfig`), and `@canon-clerk/configuration` acts as the host adapter.
- **Alternatives Considered:** Ambient resolution inside `core` (rejected due to violation of explicit dependency principles and environmental coupling).

### 2. OS Configuration Store with 0o600 Permissions vs In-Workspace Secrets
- **Decision:** Store user credentials in standard OS user configuration paths (`~/.config/canon-clerk/config.json`) with `0o600` permissions (`-rw-------`).
- **Rationale:** Agent tool sandboxes constrain file operations to the repository workspace and temporary directories. Keeping secrets in user OS storage isolates them from AI agent file-reading tools while remaining directly accessible to developer workflows.
- **Alternatives Considered:** In-workspace `.env` / `.env.local` (rejected due to agent token exfiltration and leakage risks).

### 3. Six-Tier Specificity Cascade with Lone-Key Provider Inference
- **Decision:** Evaluate configuration along a strict priority order: explicit overrides → tier-specific environment → blanket environment → vendor environment → OS credential store → static defaults. If only a provider API key is provided, infer the provider and cascade default models.
- **Rationale:** Enables zero-friction setup for users who simply provide an API key, while guaranteeing that explicit command flags and tier-specific overrides maintain absolute precedence.
- **Alternatives Considered:** Flat key lookup without tier distinction (rejected because screener and auditor tiers have distinct operational and latency profiles).

### 4. Multi-Key Ambiguity Detection with Advisory Warnings
- **Decision:** When multiple competing provider credentials exist in storage without an explicit model selection, fall back to the canonical default (`google`) and emit an advisory warning.
- **Rationale:** Avoids silent non-deterministic selection when users have multiple provider keys configured, guiding the user to declare an explicit model choice while maintaining forward compatibility.
- **Alternatives Considered:** Throwing an unhandled exception (rejected because missing configuration should fail soft where a viable default exists).

## Risks / Trade-offs

- **Cross-Platform Storage Variations:** Operating systems place configuration files in different standard directories (Linux XDG, macOS Library Preferences, Windows AppData).
  - *Mitigation:* Rely on standard platform abstraction libraries (`conf`, `env-paths`) to resolve canonical paths deterministically.
- **Permission Support on Non-POSIX Platforms:** Windows filesystems do not natively support POSIX `0o600` octal modes.
  - *Mitigation:* Apply `configFileMode: 0o600` where supported, and ensure read/write operations fail safe without throwing on permission translation mismatches.
- **ESM / CJS Bundling Collisions:** Bundling Node-specific packages into ESM can inadvertently inline CommonJS shims.
  - *Mitigation:* Externalize all dependencies in bundler configurations and align with monorepo packaging standards.
