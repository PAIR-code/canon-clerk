# Statutory Apprisal (`apprise`)

**Status:** Authoritative Architectural Standard  
**Core Domain Engine:** Caseload Domain Engine  
**Driving Adapter:** CLI (`apprise`)

---

## 1. Domain Concept & Role (`core`)

`apprise` performs **Prospective Jurisprudential Screening: Statutory Apprisal for Design-Time Guidance** across all candidate canons matching the prospective target scope.

In the court clerkship taxonomy, `apprise` represents the court exercising its **Apprisal Authority** (*Jurisdictio Notificatoria*). Rather than functioning as a trial judge hearing evidence on past infractions (the contentious `audit` path), `apprise` acts as the **Clerk of the Court apprising parties of governing rules**. It receives a prospective design inquiry—formulated as planned file targets and design intent—and determines which canons claim subject-matter jurisdiction over the planned work, providing contributors and AI coding agents with pre-flight notice of governing statutes before implementation begins.

- **Imperative Verb:** `apprise`
- **Court Clerkship Role:** Statutory apprisal, procedural notice, and jurisdictional applicability determination.
- **Metric Pair:** `apprisal_score` (number [0.0, 1.0]) and `apprisal_summary` (string rationale justifying prospective jurisdiction).
- **Core Question:** *"Given this prospective design intent and target scope, does this canon have a colorable claim of jurisdiction over the planned work?"*

---

## 2. Dependencies & Prerequisites (`core`)

```mermaid
flowchart LR
    subgraph Inputs["Inputs"]
        Validate["validate (Branch A)<br/><code>.validation</code>"]
        Config["configure (Branch B)<br/><code>.config</code>"]
        Params["Stage Flags<br/><i>(--apprise-threshold, --intent, --screener-model)</i>"]
    end

    Validate --> Apprise["apprise<br/><b>(Current Node)</b><br/><code>.apprisal</code>"]
    Config --> Apprise
    Params --> Apprise

    Apprise --> Notice["Statutory Notice Report<br/><i>(Applicable canons & guidance, Exit 0)</i>"]
    Apprise -. "zero candidates" .-> EmptyNotice["Fast Exit (No-op 0)<br/><i>Empty apprisal assessment</i>"]
    Apprise -. "missing intent or scope" .-> UsageError["Usage Error<br/><i>(Dual-input guard, Exit 2)</i>"]

    style Apprise fill:#1f6feb,stroke:#58a6ff,stroke-width:2px,color:#fff
    style EmptyNotice stroke-dasharray: 5 5
    style UsageError stroke-dasharray: 5 5
```

- **Direct Prerequisites:**
  - `validate` (Branch A: validated candidate canons in `caseload.discovery.candidate_canons` and `caseload.validation`).
  - `configure` (Branch B: resolved provider credentials and model specifiers in `caseload.config`).
- **Transitive Prerequisites:** `intake`, `discover`.
- **The Dual-Input Invariant:** Statutory apprisal evaluates governing statutes against **both** a non-empty prospective design `intent` (`--intent <text>` or pre-populated `.intake.intent`) **and** an explicit target or canon scope (`TARGETS`, `-`, `--diff`, `--all-targets`, `--all-canons`, `--canon`, or pre-populated upstream `.discovery`). Omitting either input fails fast with exit code `2` rather than silently evaluating an empty scope.
- **Branch Independence & Pruning:**
  - Evaluates without requiring code diffs, patches, or work-in-progress (WIP) exhibits.
  - Decoupled from the contentious dispute track: completely prunes `docket`, `admit`, and `audit` from execution.
  - Diagnostic leaf `probe` is never scheduled.

---

## 3. Caseload Contribution & Data Plane Constraints

### Caseload Delta
`apprise` populates the `.apprisal` namespace on the cumulative `Caseload` envelope with prospective statutory notice records:

- **Per-Canon Apprisal Assessments (`assessments`):** A deterministic map keyed by candidate canon identifier. Each assessment captures the model's `apprisal_summary` (rationale explaining why the canon does or does not govern the planned design intent and target scope), continuous `apprisal_score` in `[0.0, 1.0]`, and deterministically derived `apprisal_status` (`"applicable"` when `apprisal_score >= threshold`, otherwise `"dismissed"`).

### Data Plane & Domain Short-Circuit Invariants
1. **Deterministic Status Derivation:** The screener model emits only `apprisal_summary` and `apprisal_score`; the domain engine deterministically evaluates the score against `--apprise-threshold` (default `0.5`) to assign `apprisal_status` (`"applicable"` vs. `"dismissed"`).
2. **Empty Candidate Short-Circuit:** If `.discovery.candidate_canons` is empty, the engine attaches an empty `.apprisal` record (`assessments: {}`) and terminates immediately with exit code `0`, dispatching zero model calls.

---

## 4. Process & Domain Logic (`core`)

1. **Dual-Input Verification & Candidate Canon Early Exit:**  
   Verifies that non-empty prospective design intent is present on `caseload.intake.intent` alongside resolved discovery scope in `caseload.discovery`. Extracts `candidate_canons` from `caseload.discovery` (resolved from prospective target paths, globs, or explicit canon selection). If `candidate_canons` is empty after evaluating an explicit target or canon scope, attaches an empty `.apprisal` record (`assessments: {}`) and short-circuits immediately with exit code `0`.
2. **Aggregate Single-Turn Screening (`gemini-flash-lite-latest`):**  
   Evaluates **all candidate canons against the prospective design intent and target scope context in a single aggregate prompt turn** using `caseload.config.screener_model`.
3. **Constrained Decoding Schema (Reason-First):**  
   Enforces structured JSON output generating `apprisal_summary` before `apprisal_score`:
   ```json
   {
     "assessments": {
       ".canons/auth/cacheing-layers-must-have-configurable-expiry.md": {
         "apprisal_summary": "Proposed round-robin dispatch introduces a dynamically updated provider cache, which must define explicit expiration policies.",
         "apprisal_score": 0.92
       },
       ".canons/auth/database-migrations-must-include-rollback-instructions.md": {
         "apprisal_summary": "The planned refactor only modifies in-memory provider dispatch and does not alter database schemas or migrations.",
         "apprisal_score": 0.05
       }
     }
   }
   ```
   Generating `apprisal_summary` before `apprisal_score` provides a chain-of-thought scratchpad, anchoring reproducible probability distributions.
4. **Deterministic Threshold Gating (`apprisal_score >= --apprise-threshold`, default: `0.5`):**  
   The core domain engine deterministically evaluates the continuous score against the threshold to assign `apprisal_status`:
   - Canons scoring $\ge 0.5$ establish prospective jurisdiction and are marked `apprisal_status: "applicable"`.
   - Canons scoring $< 0.5$ are marked `apprisal_status: "dismissed"`.
5. **Latency & Token Economy:** Executes in ~400ms, expending only ~1,200 tokens across 20+ candidate canons.

---

## 5. Driving Adapter: CLI

The CLI exposes `apprise` as the primary entry point for design-time and pre-flight planning:

```bash
# 1. Inline design intent with prospective target paths:
canon-clerk apprise --intent "Refactor authentication to round-robin between providers" "src/auth" "src/frontend"

# 2. Streaming newline-delimited prospective target paths via standard input ('-'):
git diff origin/main --name-only | canon-clerk apprise --intent "Rotate auth provider" -

# 3. Running against an upstream Caseload file:
canon-clerk apprise --caseload upstream-discovery.json

# 4. Tab-separated compact records for shell scripting:
canon-clerk apprise --intent "Add CLI output flag" src/cli --format compact

# 5. Machine-readable JSON output for AI Agent prompt injection:
canon-clerk apprise --intent "Add Kafka event bus" src/events --json
```

### Dual-Input Guard (Missing Intent or Target Scope)
`canon-clerk apprise` enforces a strict **Dual-Input Guard**: invoking `apprise` requires **both** a non-empty prospective design `intent` (`--intent <TEXT>` or pre-populated `.intake.intent`) **and** an explicit target or canon scope (positional `TARGETS`, newline-delimited target paths via `-`, `--diff`, `--all-targets`, `--all-canons`, `--canon`, or pre-populated upstream `.discovery`). Omitting either input (naked invocation, supplying `--intent` without target/canon scope, or supplying target/canon scope without `--intent`) fails fast with exit code `2` (Usage Error) and prints actionable remediation guidance:
```text
error: Prospective apprisal requires both a non-empty design intent and an explicit target or canon scope.
  Hint: Pass '--intent <TEXT>' alongside target paths ('src/...', '-', '--diff <PATH|->', '--all-targets')
        or canon scope ('--all-canons', '--canon <PATH|GLOB>'), or supply '--caseload <PATH|->'.
```

Conversely, when both prospective design intent and an explicit target or canon scope are provided and `discover` matches zero candidate canons, the command attaches an empty `.apprisal` record (`assessments: {}`) and short-circuits cleanly with exit code `0` without invoking `validate` or requiring provider credentials (`GEMINI_API_KEY`).

### CLI Flags & Environment
- `TARGETS` / `-` (Positional): Prospective target file paths, directories, globs, or `-` to read newline-delimited target paths from standard input.
- `--intent <TEXT>`: Inline prospective architectural design intent or plan description.
- `--diff <PATH|->`, `--all-targets`, `--pr-title <TEXT>`, `--pr-body <TEXT>`, `--pr-body-file <PATH>`: Shared filing source and metadata flags.
- `--all-canons`, `--canon <PATH|GLOB>`: Shared canon selection scope flags.
- `--apprise-threshold <FLOAT>` (fallback env: `CANON_CLERK_APPRISE_THRESHOLD`): Prospective applicability threshold in `[0.0, 1.0]` (default: `0.5`). Ambiguous unqualified `--threshold` and out-of-closure stage threshold flags are rejected with exit code `2` and guidance pointing to `--apprise-threshold`.
- `--screener-model <MODEL>` (fallback env: `CANON_CLERK_SCREENER_MODEL`), `--auditor-model <MODEL>` (fallback env: `CANON_CLERK_AUDITOR_MODEL`), `--reasoning-budget <TOKENS>` (fallback env: `CANON_CLERK_REASONING_BUDGET`), `--gemini-api-key <KEY>` (fallback env: `GEMINI_API_KEY`): Shared model configuration and credential options (resolved JIT only when validated candidate canons exist).
- `--format <stylish|json|compact>`: Output presentation format (default: `stylish`).
- `--caseload <PATH|->`: Ingests upstream `Caseload` JSON from file or `-` for standard input.
- `--output-caseload <PATH>`: Writes cumulative `Caseload` JSON to disk independently of terminal `stdout`.
- `--json`: Emits enriched `Caseload` JSON to `stdout` (equivalent to `--format json`).

### CLI Output Modes
- **`stylish` (Default):** Renders the **Pipeline Funnel Provenance Receipt** followed by the Statutory Apprisal summary listing applicable canons alongside their `apprisal_summary` rationales.
- **`compact`:** Emits unadorned tab-separated `<canon>\t<status>\t<score>\t<summary>` records (one candidate canon per line in deterministic lexicographical order).
- **`json` / `--json`:** Emits the cumulative `Caseload` JSON envelope containing the populated `.apprisal` container.

### CLI Exit Codes
- **0:** Successful statutory apprisal (including clean zero-candidate short-circuits).
- **1:** Candidate canon validation failure (`validation.has_errors` is `true`, evaluated before checking `GEMINI_API_KEY`) or local workspace I/O runtime error.
- **2:** Usage error, dual-input guard violation (missing design intent or missing target/canon scope), invalid or out-of-range `--apprise-threshold` / `CANON_CLERK_APPRISE_THRESHOLD`, rejected `--threshold` flag, conflicting `stdin` operands, missing API credentials, or model provider error.

