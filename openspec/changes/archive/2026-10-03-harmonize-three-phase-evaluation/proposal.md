# Proposal

## Why
As established in Epic #168 and sanctioned in Issue #171, Canon Clerk's review gating architecture has evolved from a numeric staging model ("Stage 0 / 1a / 1b / 2") into the domain-partitioned **Three-Phase Evaluation Cascade: Check → Docket → Audit**:
1. **Phase 1: Check (MUST be deterministic · 0 tokens):** Intake validation and file/path boundaries (`check-canons`, `check-triggers`).
2. **Phase 2: Docket (MAY use AI · Fast/aggregate triage):** Jurisdiction screening without pass/fail compliance judgment (`docket-canons`, `docket-targets`).
3. **Phase 3: Audit (WILL use AI · Deep reasoning):** Substantive adjudication against the canon tetrad, exceptions, and line annotations (`audit`).

Currently, the repository still reflects legacy "Stage 0/1/2" terminology across `SPEC.md`, `README.md`, `AGENTS.md`, and internal docs. Furthermore, Phase 1 commands lack verb prefix parity: `check-triggers` adheres to `check-*`, while static canon validation is named `canon-clerk lint`. In accordance with `.canons/versioning/pre-v1-refactors-must-omit-backwards-compatibility-shims.md`, pre-v1 (`0.y.z`) refactors must cleanly excise retired terminology rather than carrying dead aliases or forwarding shims.

## What Changes
1. **CLI Command Parity (`check-canons`):**
   - Introduce `canon-clerk check-canons` in `packages/cli` matching the Phase 1 verb taxonomy, pairing with `check-triggers`, and mirroring Phase 2 `docket-canons`.
   - Cleanly excise the retired `lint` subcommand with zero backward-compatibility aliases per `pre-v1-refactors-must-omit-backwards-compatibility-shims`.
   - Update `package.json` scripts (`npm run check-canons`, `npm run check`) and CLI test suites.
2. **Architectural Blueprint Publication:**
   - Formalize and publish the Three-Phase evaluation cascade specification to `docs/architecture/evaluation-cascade.md`.
   - Update `docs/architecture.md` to align with the new nomenclature and link to the evaluation cascade blueprint.
3. **Terminology & Documentation Harmonization:**
   - Sweep `README.md`, `AGENTS.md`, `SPEC.md`, `llms.txt`, and internal code comments to replace legacy "Stage 0/1/2" terminology with "Phase 1: Check", "Phase 2: Docket", and "Phase 3: Audit".
4. **OpenSpec Living Spec Alignment:**
   - Reconcile living specs (`openspec/specs/`) with Check/Docket/Audit nomenclature, introducing `cli/check-canons` and retiring `cli/lint`.

## Capabilities
### New Capabilities
- `cli/check-canons`: Static validation subcommand (`canon-clerk check-canons`) verifying repository canons against syntax, schema, and structural rules at 0 tokens.

### Modified Capabilities
- `cli`: Updates root subcommand dispatch, help screens, and error hints to route `check-canons` instead of `lint`.
- `cli/check-triggers`: Reconciles purpose and requirement statements from "Stage 0" to "Phase 1: Check".
- `query-canons`: Reconciles purpose and requirement statements from "Stage 0" to "Phase 1: Check".

### Removed Capabilities
- `cli/lint`: Excised in accordance with `pre-v1-refactors-must-omit-backwards-compatibility-shims`.

## Impact
- **Packages:** `@canon-clerk/cli`, monorepo root scripts (`package.json`).
- **Documentation:** `docs/architecture/evaluation-cascade.md` (new), `docs/architecture.md`, `SPEC.md`, `README.md`, `AGENTS.md`, `llms.txt`.
- **Specs:** `openspec/specs/cli/`, `openspec/specs/query-canons/`.
