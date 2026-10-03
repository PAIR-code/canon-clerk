# Design

## Context
Canon Clerk's review gating architecture evolved in Epic #168 from a numeric staging pipeline (Stage 0/1/2) to a domain-partitioned Three-Phase Evaluation Cascade:
1. **Phase 1: Check:** Deterministic intake and boundary evaluation (0 tokens, `check-canons`, `check-triggers`).
2. **Phase 2: Docket:** Jurisdiction and relevance triage without compliance verdicts (fast/aggregate LLM, `docket-canons`, `docket-targets`).
3. **Phase 3: Audit:** Substantive compliance adjudication, exceptions, and line-level annotations (frontier reasoning LLM, `audit`).

Issue #171 mandates harmonizing repository terminology, publishing the formal cascade architecture specification to `docs/architecture/evaluation-cascade.md`, and bringing Phase 1 CLI commands into strict verb-prefix parity (`check-canons` alongside `check-triggers`).

Per `.canons/versioning/pre-v1-refactors-must-omit-backwards-compatibility-shims.md`, pre-v1 software (`0.y.z`) must cleanly excise retired terminology rather than carrying dead aliases. Therefore, `lint` is cleanly rebranded to `check-canons` with zero transitional shims.

## Goals / Non-Goals
**Goals:**
- Implement `canon-clerk check-canons` in `@canon-clerk/cli` with complete option, flag, formatting, and exit-code parity with the former `lint` command.
- Completely excise the retired `lint` subcommand and alias in accordance with the pre-v1 refactors canon.
- Update root `package.json` scripts (`check-canons`, `npm run check`) and monorepo references.
- Formalize and publish the Three-Phase Evaluation Cascade architecture specification to `docs/architecture/evaluation-cascade.md`.
- Harmonize terminology across `SPEC.md`, `README.md`, `AGENTS.md`, `llms.txt`, and living OpenSpec specifications.
- Maintain 100% test suite passage.

**Non-Goals:**
- Implementing Phase 2 (`docket-canons` / `docket-targets`) or Phase 3 (`audit`) engines (tracked in separate issues #169, #170).
- Retaining backward-compatibility aliases for `lint` (expressly forbidden by pre-v1 refactors canon).
- Altering the underlying schema validation rules or `@canon-clerk/schema` AST parsing algorithms.

## Decisions
### 1. Primary Command Naming: `check-canons`
- **Decision:** Rebrand `canon-clerk lint` to `canon-clerk check-canons`.
- **Rationale:** Aligns with Phase 1 verb taxonomy (`check-*`), follows natural `check-<noun>` grammar matching `check-triggers`, and establishes direct architectural symmetry with Phase 2's `docket-canons`. It eliminates confusion with repository code linters (ESLint, Biome).
- **Alternatives Considered:**
  - `check-lint`: Verb-verb construct (`check` + `lint`) that still carries the word "lint".
  - `check-format`: Highly misleading in developer tooling where "format" denotes Prettier/rustfmt whitespace pretty-printing.

### 2. Zero-Shim Clean Cut
- **Decision:** Excise `lint` completely from CLI command registration and npm scripts, rather than preserving an alias.
- **Rationale:** Canon Clerk is at `0.1.0`. [`.canons/versioning/pre-v1-refactors-must-omit-backwards-compatibility-shims.md`](.canons/versioning/pre-v1-refactors-must-omit-backwards-compatibility-shims.md) strictly forbids preserving transitional shims or aliases for renamed domain concepts in pre-v1 packages.

### 3. Architecture Blueprint Publication
- **Decision:** Publish the formal architecture document to `docs/architecture/evaluation-cascade.md` and interlink from `docs/architecture.md`, `SPEC.md`, and `README.md`.
- **Rationale:** Preserves `docs/architecture.md` as the high-level architecture overview while hosting the normative multi-phase cascade contract and `FileArtifact` abstraction in `docs/architecture/evaluation-cascade.md`.

## Risks / Trade-offs
- **Risk:** Developers or scripts relying on `canon-clerk lint` or `npm run lint:canons` will encounter command-not-found errors.
  - **Mitigation:** Clear error output from Commander and updated npm scripts in `package.json` (`npm run check-canons`, `npm run check`).
