# Evidence Admissibility Triage (`admit`)

**Status:** Authoritative Architectural Standard  
**Core Domain Engine:** Caseload Domain Engine  
**Driving Adapter:** CLI (`admit`)

---

## 1. Domain Concept & Role (`core`)

`admit` performs **Micro Triage: Evidentiary Relevance Screening** for each active Case on the docket. In the court clerkship taxonomy, it represents the clerk reviewing tendered documents, affidavits, and physical exhibits to formally admit only relevant evidence into the case record prior to trial.

- **Imperative Verb:** `admit`
- **Court Clerkship Role:** Micro triage establishing evidentiary admissibility.
- **Metric Pair:** `admissibility_score` (number [0.0, 1.0]) and `admissibility_summary` (string rationale).
- **Core Question:** *"For an active Case, is this candidate exhibit (file modification, target file snapshot, or inspected pull request metadata plane) admissible as relevant evidence?"*

---

## 2. Dependencies & Prerequisites (`core`)

```mermaid
flowchart LR
    subgraph Inputs["Inputs"]
        Docket["docket (or --caseload)<br/><code>.docket</code>"]
        Params["Stage Flags<br/><i>(--admit-threshold, --admit-concurrency,<br/>--docket-threshold, --screener-model)</i>"]
    end

    Docket --> Admit["admit<br/><b>(Current Node)</b><br/><code>.evidence</code>"]
    Params --> Admit

    Admit --> Vet["vet (Prima Facie Vetting)<br/><i>(Pipeline Cascade)</i><br/><code>.vetting</code>"]
    Admit -. "standalone CLI" .-> AdmitReport["Evidence Manifest<br/><i>(Admitted exhibits per case, Exit 0)</i>"]
    Admit -. "empty docket or zero admitted exhibits" .-> ZeroExhibits["Fast Exit (No Evidence 0)<br/><i>Prunes vet & audit</i>"]
    Admit -. "missing filing" .-> UsageError["Usage Error<br/><i>(Case-or-Controversy guard, Exit 2)</i>"]

    style Admit fill:#1f6feb,stroke:#58a6ff,stroke-width:2px,color:#fff
    style AdmitReport stroke-dasharray: 5 5
    style ZeroExhibits stroke-dasharray: 5 5
    style UsageError stroke-dasharray: 5 5
```

- **Direct Prerequisites:**
  - `docket` (active macro triage docket in `caseload.docket.active_docket`).
  - `configure` (Branch B: resolved provider credentials and model specifiers in `caseload.config`, evaluated JIT only when active cases with candidate exhibits exist).
- **Transitive Prerequisites:** `intake`, `discover`, `validate`.
- **The Case or Controversy Invariant:** Like `docket`, `admit` evaluates the admissibility of concrete filing exhibits *as applied to an active case*. It strictly requires an underlying filing (`caseload.intake` populated from target paths, `-`, `--diff`, `--all-targets`, PR metadata, or an incoming `--caseload`). Naked invocations or canon-only invocations lacking a concrete filing (e.g. `canon-clerk admit --all-canons` without targets or diff) fail fast with exit code `2`.
- **Pruned from Execution:** `probe`, `apprise`, downstream compliance vetting and substantive adjudication (`vet`, `audit`).

---

## 3. Caseload Contribution & Data Plane Constraints

### Caseload Delta
`admit` populates the `.evidence` namespace on the cumulative `Caseload` envelope with micro evidentiary relevance assessments across all docketed cases:

- **Per-Case Exhibit Assessments (`exhibits`):** A deterministic map keyed by active canon identifier (`canon_path`). Each active case maps to a stably sorted list of exhibit assessments (`file_path`, `admissibility_summary`, continuous `admissibility_score` in `[0.0, 1.0]`, and deterministically derived `admissibility_status` of `"admitted"` when `admissibility_score >= threshold` or `"excluded"` when below threshold). Here `file_path` identifies either a repository-relative POSIX file path or an inspected filing metadata plane (`"pr_title"`, `"pr_body"`, `"linked_issues"`, `"commit_messages"`).
- **Trial-Ready Active Cases (`active_cases`):** Stably sorted list of canon identifiers from `.docket.active_docket` that retain at least one `"admitted"` exhibit after threshold gating, establishing the exact cases that proceed to prima facie vetting (`vet`) and substantive adjudication (`audit`).
- **Diagnostic Anomalies (`anomalies`):** Optional manifest recording any non-fatal evidentiary screening anomalies encountered during micro triage (omitted when empty).

### Data Plane & Domain Short-Circuit Invariants
1. **Auditability with Strict Admitted-Only Isolation:** Preserving `"excluded"` assessments alongside `"admitted"` assessments in `.evidence.exhibits` provides complete provenance in `caseload.json`, `--format compact`, and multi-trial consistency benchmarks without lossy score truncation. Downstream `vet` and `audit` evaluations consume exclusively the admitted-only exhibit projection (`admissibility_status == "admitted"`), structurally guaranteeing that excluded exhibits never enter compliance prompts.
2. **Unified File & Metadata-Plane Screening vs. Grounding References:** `admit` screens contested filing exhibits—both candidate file exhibits (unified diff patches or materialized workspace target files within the canon's monorepo `<scope>/`) and present `inspect:` metadata planes (`pr_title`, `pr_body`, `linked_issues`, `commit_messages`). Static repository `references:` declared in canon frontmatter are authoritative statutory context injected during `vet` and `audit`, not contested filing exhibits screened for exclusion.
3. **Completion-Order-Independent Deterministic Folding:** Concurrent per-case and multi-slice screening calls fold disjoint slice exhibit assessments into a lexicographically ordered `exhibits` map (with each case's exhibit list sorted lexicographically by `file_path`) and stably sorted `active_cases`, yielding byte-for-byte identical `.evidence` envelopes regardless of slice count or in-flight worker completion order.
4. **Empty-Docket Short-Circuit:** If `.docket.active_docket` is empty (either from upstream macro dismissal or a pre-populated `--caseload`), `admit` attaches an empty `.evidence` record (`exhibits: {}`, `active_cases: []`) and exits `0` immediately with zero model calls and without requiring provider credentials when `.docket` was pre-populated.
5. **Zero-Evidence Short-Circuit:** If every case on `.docket.active_docket` retains zero admitted exhibits (`active_cases` is empty), the engine attaches the populated `.evidence` record and halts downstream cascade execution immediately with exit code `0`, scheduling zero `vet` or `audit` evaluations.

---

## 4. Process & Domain Logic (`core`)

1. **Prerequisite Verification & Empty-Docket Early Exit:**  
   Verifies that `.intake`, `.discovery`, and `.docket` are present and that static canon validation succeeded (`.validation.has_errors` is `false`). If `.docket.active_docket` is empty, attaches an empty `.evidence` record and returns immediately with zero model calls.
2. **Candidate Exhibit Resolution per Active Case:**  
   For each canon on `.docket.active_docket`, resolves its stably sorted candidate exhibits:
   - In standard triggered discovery, candidate exhibits come directly from `.discovery.triggers_join[canon_path]` (unifying matched file paths and active `inspect:` metadata planes).
   - In plenary `--all-canons` discovery (where trigger matching was bypassed), candidate exhibits resolve from in-scope `.discovery.target_files` (when the canon inspects `diff`) unioned with present filing metadata planes declared in the canon's `inspect:` frontmatter.
   - Candidate file exhibits present unified diff patches from `.intake.diffs` when available, or materialized UTF-8 file snapshots from the workspace when evaluating target-path or `--all-targets` sweeps.
3. **Dual-Paradigm Evidentiary Screening (Discriminative Shared-State Batching vs. Context- & Count-Sliced Generative):**  
   Evaluates active cases using `caseload.config.screener_model`, dispatching by model paradigm rather than provider prefix:
   - **Discriminative Shared-State Batching (`google:gemini-decision-*`, e.g., default `gemini-decision-1`; `typesafe:jev-*`, e.g., `jev-latest`):** Packs `(canon, exhibit)` admissibility hypotheses (`nouls`) across active cases into context-budgeted shared-state batches governed by per-model decision budget profiles (`480,000`-character batch / `256` `nouls` per call on `google:gemini-decision-*`; `64,000`-character batch / `48` `nouls` per call on `typesafe:jev-*`) so each batch encodes its union of candidate exhibits once rather than re-encoding multi-file diffs per case, executing batches concurrently up to `--admit-concurrency` with zero output tokens and zero cross-question interference.
   - **Generative Per-Case & Context-Sliced Screening (generative `google:*`, e.g., `gemini-4-hydrogen`):** Partitions each active case's resolved candidate exhibits (in canonical lexicographical order, independently of the governing canon body) into disjoint, non-overlapping slices bounded by **both** a prompt character ceiling (`60,000` characters, `~15k` tokens) and a per-slice exhibit count ceiling (`15` exhibits/slice)—admitting any single oversized exhibit exceeding the character budget as a singleton slice so no candidate exhibit is ever dropped, while fast-pathing single-slice cases with zero prompt change. Multi-slice prompts repeat the shared `Filing Overview` header (`PR Title`, bounded `PR Body` and `Linked Issues` metadata context, and the complete numbered manifest of filing file paths in scope) on every slice so contingent commit-type preconditions (`fix:` vs. `feat:`) and companion-artifact omission checks (`README.md` / living spec updates) retain identical global filing visibility across slices, while scoping `Candidate Exhibits` and `Required file_path Identifiers` strictly to that slice's disjoint subset and keeping `Filing Overview` + `Candidate Exhibits` ahead of `Active Case Governing Canon` so sibling canons sharing identical exhibit sets share byte-identical prefixes per slice for provider prefix caching. Flattens all prepared case-slice turns across the shared `--admit-concurrency` / `--max-concurrent-calls` worker pool (default `4`) at `temperature: 0.0` (with bounded minimal/zero screener reasoning enforced—`thinkingLevel: "LOW"` with `includeThoughts: false` on Gemini 4 models and `thinkingBudget: 0` on Gemini 2.5/Flash-Lite models) and deterministically folds disjoint slice assessments back into `.evidence.exhibits` in canonical `file_path` order.
   - **Telemetry Aggregation & Contextual Fail-Fast:** Dispatches calls over the shared pooled provider transport ([`../transport.md`](../transport.md)), emitting `ProviderRetry`, per-call `ModelCall` (recording the active canon identifier and `[slice k/N]` label on multi-slice generative calls), and incremental `StageProgress` telemetry while summing token accounting and thought events into `StageFinish`; unrecoverable or retry-exhausted call failures fail fast and annotate the failing canon identifier (and `(slice k/N)` when multi-slice) directly in the error diagnostic.
4. **Constrained Decoding Schema (Domain-Indirected, Reason-First):**  
   Enforces a constant-size structured JSON wire array pairing each `file_path` (file path or metadata plane identifier) with `admissibility_summary` generated before `admissibility_score`:
   ```json
   {
     "exhibits": [
       {
         "file_path": "pr_title",
         "admissibility_summary": "Conventional Commit `feat(cli):` prefix is irrelevant to kebab-case flag naming.",
         "admissibility_score": 0.12
       },
       {
         "file_path": "src/cli/docket/mod.rs",
         "admissibility_summary": "Defines new `--docket-threshold` CLI flag governed by kebab-case naming rules.",
         "admissibility_score": 0.95
       }
     ]
   }
   ```
   Generating `admissibility_summary` before `admissibility_score` provides a chain-of-thought scratchpad anchoring reproducible probability distributions. Every candidate exhibit assigned to the evaluated case or slice must appear once without unknown or duplicate identifiers.
5. **Anchored Evidentiary Rubric & Omission Awareness (`admissibility_score >= --admit-threshold`, default: `0.5`):**  
   Screener system instructions anchor scores to discrete relevance bands (`0.75 - 0.95` for relevant evidence vs. `0.05 - 0.25` for orthogonal boilerplate), separating **evidentiary relevance** from **substantive compliance** (compliant code touching the governed construct is still relevant evidence and must be admitted). For canons requiring companion updates or metadata (such as `feat` pull requests required to update `README.md` or link motivating issues), exhibits evidencing the triggering change (`pr_title`, `pr_body`, or modified feature files) are scored admissible even when the required companion file is absent from the diff. The domain engine then deterministically derives `admissibility_status`:
   - Exhibits scoring $\ge 0.5$ are marked `"admitted"`.
   - Exhibits scoring $< 0.5$ are marked `"excluded"`.
   - Cases retaining $\ge 1$ admitted exhibit enter `active_cases`; cases with zero admitted exhibits are dismissed without trial.

---

## 5. Driving Adapter: CLI

The CLI exposes `admit` as an imperative subcommand:

```bash
# 1. Run telescoping pipeline stopping at admit from a unified diff on stdin:
git diff origin/main | canon-clerk admit --diff -

# 2. Screen explicit target paths or newline-delimited stdin paths:
canon-clerk admit src/cli src/docket
git diff origin/main --name-only | canon-clerk admit -

# 3. Execute micro evidentiary triage against an upstream Caseload:
canon-clerk admit --caseload caseload-5.json --json

# 4. Emit tab-separated compact exhibit assessments for shell scripting:
git diff origin/main | canon-clerk admit --diff - --format compact
```

### Missing Input Source & Case-or-Controversy Guard
`canon-clerk admit` requires an underlying filing (positional `TARGETS`, newline-delimited target paths via `-`, `--diff <PATH|->`, `--all-targets`, pull request metadata flags, or an incoming `--caseload <PATH|->` with `.intake` populated). Naked invocations or canon-only invocations lacking a concrete filing (e.g. `canon-clerk admit --all-canons` without target paths or diff, or an abstract corpus-only `--caseload` without `.intake`) fail fast with exit code `2` (Usage Error) and print actionable remediation guidance:
```text
error: No filing source provided for `admit`.
  Hint: Pipe a diff via standard input ('--diff -'), specify target paths ('src/...', '-', '--all-targets'),
        or pass an upstream caseload ('--caseload <PATH|->') with '.intake' populated.
```
Conversely, when an explicitly designated stream yields zero changes, `discover` matches zero candidate canons, or `docket` opens zero colorable cases, the pipeline short-circuits cleanly with exit code `0`.

### CLI Flags & Environment
- `TARGETS` / `-` (Positional): Target file paths, directories, globs, or `-` to read newline-delimited target paths from standard input.
- `--diff <PATH|->`, `--all-targets`, `--intent <TEXT>`, `--pr-title <TEXT>`, `--pr-body <TEXT>`, `--pr-body-file <PATH>`: Shared filing source and metadata flags.
- `--all-canons`, `--canon <PATH|GLOB>`: Shared canon selection scope flags (evaluated against the underlying filing).
- `--admit-threshold <FLOAT>` (fallback env: `CANON_CLERK_ADMIT_THRESHOLD`): Evidentiary admissibility threshold in `[0.0, 1.0]` (default: `0.5`). Ambiguous unqualified `--threshold` and out-of-closure stage flags (`--apprise-*`, `--vet-*`, `--audit-*`) are rejected with exit code `2` and guidance pointing to `--admit-threshold` and `--docket-threshold`.
- `--admit-concurrency <COUNT>` / `--max-concurrent-calls <COUNT>` (fallback env: `CANON_CLERK_ADMIT_CONCURRENCY` / `CANON_CLERK_MAX_CONCURRENT_CALLS`; config file key: `admitConcurrency`): Optional positive integer (`>= 1`) bounding the maximum concurrent in-flight per-case / per-slice screener calls (default: `4`). Follows standard precedence (explicit CLI flag $\to$ environment variable $\to$ `~/.config/canon-clerk/config.json` $\to$ built-in default).
- `--docket-threshold <FLOAT>` (`CANON_CLERK_DOCKET_THRESHOLD`), `--docket-batch-size <COUNT>` / `--max-canons-per-call <COUNT>` (`CANON_CLERK_DOCKET_BATCH_SIZE`), `--docket-concurrency <COUNT>` (`CANON_CLERK_DOCKET_CONCURRENCY`): Upstream macro jurisdiction triage options accepted transitively for telescoping runs.
- `--screener-model <MODEL>` (`CANON_CLERK_SCREENER_MODEL`), `--auditor-model <MODEL>` (`CANON_CLERK_AUDITOR_MODEL`), `--thinking-level <minimal|low|medium|high>` (`CANON_CLERK_THINKING_LEVEL`), `--reasoning-budget <TOKENS>` (`CANON_CLERK_REASONING_BUDGET`), `--gemini-api-key <KEY>` (`GEMINI_API_KEY`), `--typesafe-api-key <KEY>` (`TYPESAFE_API_KEY`): Shared model configuration and credential options (resolved JIT only when heuristic screening is required).
- `--max-retries <COUNT>` (`CANON_CLERK_MAX_RETRIES`; config key: `maxRetries`, default: `2`), `--max-backoff-ms <MS>` (`CANON_CLERK_MAX_BACKOFF_MS`; config key: `maxBackoffMs`, default: `90000`): Shared pooled transport retry and per-origin rate-limit cooldown bounds ([`../transport.md` §6](../transport.md#6-out-of-band-telemetry--cli-transport-controls)).
- `-v` / `--verbose` (`CANON_CLERK_VERBOSE`), `--log-file <PATH>` (`CANON_CLERK_LOG_FILE`): Global diagnostic progress and structured JSONL telemetry stream options.
- `--format <stylish|json|compact>`: Output presentation format (default: `stylish`).
- `--caseload <PATH|->`: Ingests upstream `Caseload` JSON from file or `-` for standard input.
- `--output-caseload <PATH>`: Writes cumulative `Caseload` JSON to disk independently of terminal `stdout`.
- `--json`: Emits enriched `Caseload` JSON to `stdout` (equivalent to `--format json`).

### CLI Output Modes
- **`stylish` (Default):** Renders the 6-stage **Pipeline Funnel Provenance Receipt** (`intake` $\to$ `discover` $\to$ `validate` $\to$ `configure` $\to$ `docket` $\to$ `admit`) followed by the Evidence Manifest listing active cases (`active_cases`) and their admitted exhibits alongside `admissibility_summary` rationales (including explicit watermarks on empty dockets or zero admitted exhibits).
- **`compact`:** Emits unadorned tab-separated `<canon>\t<exhibit>\t<status>\t<score>\t<summary>` records in deterministic lexicographical order by `(canon_path, file_path)`.
- **`json` / `--json`:** Emits the cumulative `Caseload` JSON envelope containing the populated `.evidence` container.

### CLI Exit Codes
- **0:** Candidate exhibits screened and `.evidence` attached (including clean zero-change, zero-candidate, empty-docket, and zero-admitted-exhibits short-circuits).
- **1:** Candidate canon validation failure (`validation.has_errors` is `true`, evaluated before checking `GEMINI_API_KEY`) or local workspace I/O runtime error.
- **2:** Usage error, missing filing source or Case-or-Controversy guard violation, invalid or out-of-range `--admit-threshold` / `--docket-threshold`, invalid or zero `--admit-concurrency` / `--docket-batch-size` / `--docket-concurrency`, invalid `--max-retries` / `--max-backoff-ms`, rejected `--threshold` or `--apprise-*` flag, conflicting `stdin` operands, missing API credentials, or model provider error.
