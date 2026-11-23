# The Caseload Pipeline: Architecture & Overview

**Status:** Authoritative Architectural Standard  
**Domain Concept:** The Caseload

---

## 1. Executive Summary: The Caseload Paradigm

Every evaluation in Canon Clerk begins with an in-flight change:
- Direct target file arguments (`canon-clerk audit packages/cli/src/app.ts`)
- Directory or file globs (`canon-clerk audit src/`, `canon-clerk audit '**/*.ts'`)
- Positional target paths streamed via stdin (`git diff origin/main --name-only | canon-clerk audit -`)
- A unified diff patch stream (`git diff origin/main | canon-clerk audit --diff -`)
- Pull request metadata (`--pr-title`, `--pr-body`) or structured payloads (`--gh-pr -`)

That in-flight change initiates a **`Caseload`**.

The `Caseload` is the central, cumulative state container flowing through a **Directed Acyclic Graph (DAG)** of functional stages. Rather than a collection of disjoint subcommands producing disparate outputs, Canon Clerk models the entire evaluation as a functional state machine where each stage enriches the shared `Caseload` record.

Subcommands serve as named **terminal stop points** along the DAG, formulated as crisp, single-word **imperative verbs**:

$$
\begin{aligned}
\text{Branch A (Filing Track):}\quad & \text{intake} \longrightarrow \text{discover} \longrightarrow \text{validate} \\
\text{Branch B (Environment Track):}\quad & \text{configure} \quad (\longrightarrow \text{probe}) \\
\text{Adjudication Spine:}\quad & \{\text{validate}, \text{configure}\} \longrightarrow \text{docket} \longrightarrow \text{admit} \longrightarrow \text{vet} \longrightarrow \text{audit} \\
\text{Apprisal Track:}\quad & \{\text{validate}, \text{configure}\} \longrightarrow \text{apprise}
\end{aligned}
$$

When invoked, a subcommand executes only the **transitive dependency closure** required for its stage, pruning unneeded branches. Downstream commands fast-forward without re-running upstream stages when supplied an accumulated `Caseload` via `--caseload <path|->`.

---

## 2. DAG Topology: Two Feeder Branches $\to$ Adjudication Spine & Apprisal Track

The DAG consists of **Two Feeder Branches** that converge at `docket` (for dispute adjudication) and `apprise` (for prospective statutory notice), followed by the **Adjudication Spine**, with an auxiliary diagnostic leaf for provider health:

1. **Branch A: The Filing Track (`intake` $\longrightarrow$ `discover` $\longrightarrow$ `validate`):**  
   Ingests in-flight diffs, target paths, or prospective design intent, verifies state preconditions (`exists:`), matches candidate canon triggers, and validates candidate canon ASTs and schemas. Completely local, deterministic, and requires 0 tokens and zero API credentials.
2. **Branch B: The Environment Track (`configure`):**  
   Resolves the active default provider profile (`--provider <slug>`), per-node `<provider>:<model>` bindings across the invoked closure (`apprise`, `docket`, `admit`, `vet`, `audit`), extended thinking controls, endpoint `baseUrl` overrides, and required provider credentials across built-in (`google`, `typesafe`) and custom `providers.<slug>` profiles. Completely offline, deterministic, and completes in <5ms with 0 tokens and zero network calls.
3. **Diagnostic Leaf (`probe`):**  
   An auxiliary termination node depending strictly on `configure`. Concurrently probes deduplicated configured model targets for live reachability and roundtrip latency without triggering an evaluation run.
4. **The Heuristic Cascade (`docket` $\longrightarrow$ `admit` $\longrightarrow$ `vet` $\longrightarrow$ `audit`):**  
   Converges at `docket` (requiring both validated candidate canons and active node configuration), followed by exhibit admissibility screening (`admit`), prima facie compliance vetting on the papers (`vet`), and isolated generative adjudication of contested cases (`audit`).
5. **The Prospective Apprisal Track (`apprise`):**  
   Converges at `apprise` (requiring validated candidate canons and `apprise` node configuration) to evaluate prospective design intent before implementation, completely bypassing diff parsing, `docket`, `admit`, `vet`, and `audit`.

```mermaid
flowchart TD
    subgraph BranchA ["Branch A: The Filing (Deterministic · 100% Local · 0 Tokens · No Credentials)"]
        InFlight["In-Flight Change<br/>(Target Paths, Globs, Diff Stream, or Intent)"] --> S1["intake<br/>(Parse diffs, target paths/globs, & PR metadata → FileArtifacts)"]
        S1 --> S2["discover<br/>(Check exists: preconditions, match triggers, & prune exhibits)"]
        S2 --> S3["validate<br/>(AST & schema pre-flight of candidate canons)"]
    end

    subgraph BranchB ["Branch B: The Environment (Deterministic · 100% Local · 0 Tokens)"]
        Env["Environment & Flags<br/>(Provider selector, per-node models, API keys, thinking controls)"] --> S4["configure<br/>(Resolve per-node models & verify closure credentials)"]
    end

    subgraph DiagnosticLeaf ["Diagnostic Leaf (Network · Latency · 0 Tokens)"]
        S4 -.-> S4_Probe["probe<br/>(Live deduplicated endpoint connectivity & latency check)"]
    end

    subgraph ApprisalTrack ["The Apprisal Track (Design-Time · Statutory Notice · Discriminator / Generator)"]
        S_Apprise["apprise<br/>(Statutory Notice: Candidate canons → Applicable Canons)"]
    end

    subgraph HeuristicCascade ["The Heuristic Cascade (AI-Driven · Positive Polarity)"]
        S5["docket<br/>(Macro Triage: Candidate canons → Active Cases)"] --> S6["admit<br/>(Micro Triage: Target files/diff hunks → Admitted Exhibits)"]
        S6 --> S6_Vet["vet<br/>(Prima Facie Vetting: Active Cases → Vetted vs. Contested)"]
        S6_Vet --> S7["audit<br/>(Judicial Adjudication: Generative trials for Contested Cases)"]
    end

    S3 --> S5
    S4 --> S5
    S3 --> S_Apprise
    S4 --> S_Apprise

    S7 --> FinalRecord["Final Caseload Record<br/>(Decree / Verdict, Line Annotations, Evidentiary Audit Trail)"]
```

---

## 3. Hexagonal Architecture: The CLI Driving Adapter vs. Core Domain Processing

Canon Clerk strictly abides by **Hexagonal Architecture (Ports & Adapters)**. The DAG evaluation model is implemented as pure, environment-agnostic domain logic, decoupled from command-line arguments, operating system process boundaries, continuous integration environments, and external AI provider wire protocols.

```mermaid
flowchart TD
    subgraph DrivingAdapter ["Driving Adapter (Inbound Port)"]
        CLI["CLI Binary<br/>(Flags, POSIX stdin Streams, Exit Codes, Progress, Output Formatting)"]
    end

    subgraph CoreEngine ["The Caseload Domain Engine"]
        Core["Domain Engine<br/>(Functional DAG Node Implementations & Scheduling)"]
        Config["Configuration & Probe<br/>(Credential Discovery & Provider Diagnostics)"]
        Schema["Schema Layer<br/>(Canon AST & Caseload Data Model)"]
    end

    subgraph DrivenAdapters ["Driven Adapters (Outbound Ports)"]
        Provider["Structured Model Provider<br/>(Deterministic JSON Decoding, Reasoning Budgets, Token & Thought Telemetry)"]
        DecisionProvider["Decision Model Provider<br/>(Discriminative Parallel Noul Probabilities, Zero Output Tokens)"]
    end

    CLI --> Core
    CLI --> Config

    Config --> Core
    Core --> Schema
    Core --> Provider
    Core --> DecisionProvider
    Config -.-> Provider
    Config -.-> DecisionProvider
```

### The Domain Engine (The Hexagon Core)
- **Schema Layer:** Defines the canonical representations for canons, frontmatter, ASTs, and the cumulative `Caseload` state container.
- **Evaluation Engine:** Houses pure implementations of all evaluation stages (`intake`, `discover`, `validate`, `docket`, `admit`, `vet`, `audit`, `apprise`), DAG scheduling algorithms, prompt assembly, and constrained decoding schemas. It has no dependencies on CLI flags, stdout formatting, external environments, or vendor-specific HTTP payloads.
- **Configuration & Probe:** Resolves the active default provider, per-node `<provider>:<model>` bindings, and API credentials across the invoked closure, and implements the diagnostic `probe` endpoint health check.

### The Driven Adapter (The Model Provider Boundary)
- **Provider-Neutral Structured Generation Port (`Generator`):** Encapsulates wire-protocol generative LLM transport (`gemini` `:generateContent` across `google` and custom `"protocol": "gemini"` profiles), REST wire types, and authentication headers behind a provider-agnostic port so heuristic stages (`apprise`, `docket`, `admit`, `vet`, `audit`) evaluate against canonical `<provider>:<model>` specifiers, system instructions, prompts, and JSON Schemas without coupling to vendor hostnames.
- **Discriminative Decision Model Port (`Discriminator`):** Encapsulates discriminative classifier wire protocols (`gemini` `POST {baseUrl}/decisions` for `<slug>:gemini-decision-*` and `systemone` `POST {normalizedBaseUrl}/systemone` for `typesafe:jev-*`, built-in keyless `local:*`, and custom `"protocol": "systemone"` profiles) behind a parallel binary-question (`Noul`) port that evaluates batched yes/no hypotheses against shared filing context in a single forward pass with zero autoregressive output tokens—powering ultra-low-latency screening across `apprise`, `docket`, and `admit`, plus prima facie compliance vetting in `vet`, under protocol- and profile-bound context, concurrency, and question-batching budget profiles.
- **Deterministic Reason-First Constrained Decoding:** Enforces deterministic generation (`temperature: 0.0`) paired with strict JSON Schema output enforcement (`application/json`) on generative models so rationale scratchpad fields precede numeric scores across generative calls.
- **Extended Reasoning & Out-of-Band Telemetry Capture:** Maps per-node discrete qualitative thinking tiers (`thinking_level`: `minimal`, `low`, `medium`, `high`) and optional token ceilings (`reasoning_budget`) to mutually exclusive provider-native extended thinking controls on `Generator`-bound nodes—enforcing bounded minimal/zero reasoning (`thinkingLevel: "LOW"` with `includeThoughts: false` on Gemini 4 models and `thinkingBudget: 0` on Gemini 2.5/Flash-Lite models) by default on generative screening/vetting nodes (`apprise`, `docket`, `admit`, `vet`) while applying bounded `"LOW"` thinking (`includeThoughts: true`) by default on `audit` trials—separates intermediate reasoning thought summaries from decoded payloads, and captures prompt, completion, reasoning, and total token accounting alongside roundtrip latency for immediate per-call and stage-summary emission on the out-of-band Telemetry Event Stream.
- **Credential Encapsulation & Actionable Error Classification:** Secret API keys are bound to concrete provider adapters at construction rather than carried on evaluation requests or `Caseload` envelopes, and upstream HTTP or transport failures (`401`/`403`, `404`, `429`, `5xx`, context token limits, timeouts, safety blocks, or malformed responses) are classified into actionable diagnostics with provider-specific remediation hints.
- **Pooled Provider Transport, Per-Origin Cooldown Coordination & Bounded Transient Retry:** Generative and discriminative provider adapters share a persistent-reactor HTTPS connection pool coordinated by per-origin `429`/`503` cooldown gates, wire-only per-attempt timeouts, 4-tier retry-delay parsing, and bounded transient retry (`--max-retries`, `--max-backoff-ms`) across heuristic evaluation stages while keeping offline closures zero-overhead and diagnostic `probe` checks strictly single-shot; see [`transport.md`](transport.md) for the authoritative transport and rate-limit scheduling specification.

### The Driving Adapter (The CLI Boundary)
- **CLI Binary:** The sole driving adapter translating POSIX stdin streams (`-`, `--diff -`, `--caseload -`), argv flags, and local working directories into inputs for the domain engine and configuration. Formats user-facing terminal progress, spinners, event streams, and the **Pipeline Funnel Receipt** (summarizing upstream ancestor outcomes to prevent "silent zero" ambiguity), and maps domain results to shell exit codes (`0`, `1`, `2`).
- **CI / GitHub Integration (Tripartite Output Model):** CI runners invoke the native CLI binary directly without requiring a separate Action codebase. The CLI natively satisfies three output tiers in GitHub Actions:
  1. **Inline Diff Annotations:** Emits native workflow command annotations (`::error file=...::`, `::warning file=...::`) for all `CodeAnnotation`s on `stdout`/`stderr`, pinpointing line violations directly on the PR *Files changed* diff.
  2. **Run Summary:** Appends the rendered Markdown Funnel Receipt and stage verdict report to `$GITHUB_STEP_SUMMARY`.
  3. **Caseload Artifact Archival:** Supports `--output-caseload <path>` to write the full-fidelity cumulative `Caseload` JSON to disk independently of stdout. This allows workflows to upload `caseload.json` via `actions/upload-artifact` (`if: always()`) for post-run failure forensics, compliance auditing, or multi-job workflow chaining:
     ```yaml
     - name: Run Canon Clerk Gate
       run: |
         git diff origin/main | canon-clerk audit --diff - --output-caseload caseload.json

     - name: Archive Caseload Record
       if: always()
       uses: actions/upload-artifact@v4
       with:
         name: canon-clerk-caseload
         path: caseload.json
         retention-days: 14
     ```

---

## 4. The Clerkship Sequence: Colorability $\to$ Admissibility $\to$ Vetting $\to$ Compliance

Grounding AI evaluation in the cognitive and procedural division of labor of a court clerkship establishes distinct, unambiguous semantics for every heuristic stage:

| Node / Imperative Verb | Core Concept | Metric Pair | Question Answered | Gate / Verdict Threshold |
| :--- | :--- | :--- | :--- | :--- |
| **`apprise`** | **Statutory Apprisal** *(Procedural Notice)* | `apprisal_score`<br/>`apprisal_summary` | *"Given this prospective design intent and target scope, does this canon have a colorable claim of jurisdiction over the planned work?"* | `score >= 0.5` $\implies$ Marked **Applicable** |
| **`docket`** | **Colorability** *(Subject-Matter Jurisdiction)* | `colorability_score`<br/>`colorability_summary` | *"Does this candidate canon have a colorable claim of jurisdiction over this PR as a whole?"* | `score >= 0.5` $\implies$ Opened as an **Active Case** |
| **`admit`** | **Admissibility** *(Relevance of Evidence)* | `admissibility_score`<br/>`admissibility_summary` | *"For an active Case, is this specific file/diff hunk admissible as relevant evidence?"* | `score >= 0.5` $\implies$ Admitted as an **Exhibit** |
| **`vet`** | **Prima Facie Vetting** *(Uncontested Compliance)* | `vetting_score`<br/>`vetting_summary` | *"Given the admitted exhibits and governing statute, does the filing clearly comply on its face without requiring remediation?"* | `score >= 0.8` $\implies$ **Vetted** (`vetted`)<br/>`score < 0.8` $\implies$ **Contested** (`contested`) |
| **`audit`** | **Compliance** *(Substantive Merits)* | `compliance_score`<br/>`compliance_summary` | *"Given the admitted exhibits and governing invariant/exceptions, does the contested change comply with canon statute?"* | `score >= 0.5` $\implies$ **Compliant** (`pass`) 🟢<br/>`score < 0.5` $\implies$ **Violation** (`fail`) 🔴 |

### The Court Clerkship Taxonomy
- **The Caseload:** The cumulative lifecycle container for the evaluation run.
- **Statutory Apprisal (`apprise`):** Procedural notice issued by the clerk apprising parties of governing statutes pertaining to prospective design intent before implementation begins.
- **Candidate Canons:** Rules whose declared `exists:` state preconditions match the Target File Tree, and whose `triggers:` or `inspect:` planes match in-flight exhibits during `discover` (subject to monorepo Scope Containment).
- **Cases:** Canons that pass macro triage during `docket` and enter the Active Docket.
- **Exhibits (The Unified Evidence Lifecycle):**
  - **Tendered Exhibits (`intake`):** Raw filing inputs, including literal text metadata (`pr_title`, `pr_body`, `commit_messages`, `linked_issues`), diff streams (`diff`), prospective design intent (`--intent`), and target file discovery directives.
  - **Candidate Exhibits (`discover`):** Materialized exhibits retained after mutual pruning with candidate canons (dropping un-inspected exhibits to preserve token hygiene).
  - **Admitted Exhibits (`admit`):** Exhibits formally admitted as relevant evidence for a specific Case on the docket (`admissibility_score >= 0.5`).
- **Prima Facie Vetting (`vet`):** Pre-trial compliance review on the papers clearing plainly compliant cases (`vetted`) and binding disputed cases (`contested`) over for trial.
- **Trial / Decree:** The isolated generative prompt turn and final adjudication rendered during `audit` per contested Case against its admitted exhibits.

### Positive Polarity Consistency
All heuristic metrics share an identical polarity convention: **a higher score reflects the affirmative presence of the named property**:
- **`apprise` (High Apprisal):** Affirmative jurisdiction $\implies$ canon marked applicable (`score >= 0.5`).
- **`docket` (High Colorability):** Affirmative jurisdiction $\implies$ canon opened as an active Case.
- **`admit` (High Admissibility):** Affirmative relevance $\implies$ file/hunk admitted as an Exhibit for that Case.
- **`vet` (High Prima Facie Compliance):** Affirmative uncontested adherence $\implies$ case cleared on the papers (`score >= 0.8`).
- **`audit` (High Compliance):** Affirmative adherence $\implies$ change complies with canon statute and passes review (`score >= 0.5`).

---

## 5. The Caseload Pipeline Nodes

| Node / Verb | Track | Engine / Capability | Cost / Latency | Gate Rule |
| :--- | :--- | :--- | :--- | :--- |
| `intake` | Branch A (Filing) | Deterministic | 0 tokens, ~5ms | Parsed context; fail fast (code 2) on corrupted input |
| `discover` | Branch A (Filing) | Deterministic | 0 tokens, ~8ms | Matches > 0; verifies `exists:` & `triggers:`; short-circuits (exit 0) on 0 candidates |
| `validate` | Branch A (Filing) | Deterministic | 0 tokens, ~12ms | 0 syntax errors; fails fast (code 1) on lint error |
| `configure` | Branch B (Env) | Deterministic | 0 tokens, <5ms | Valid node config & closure keys; fails fast (code 2) on invalid flags or missing keys |
| `probe` | Diagnostic Leaf | Network probe | 0 tokens, variable | Deduplicated targets reachable; fails fast (code 2) on failure |
| `apprise` | Apprisal Track | `Discriminator` / `Generator` (`google:gemini-decision-1` / `typesafe:jev-latest` / `google:gemini-4-hydrogen`) | ~150–400ms, 0–low \$ | `apprisal_score >= 0.5`; exits 0 if candidates empty |
| `docket` | Cascade Spine | `Discriminator` / `Generator` (`google:gemini-decision-1` / `typesafe:jev-latest` / `google:gemini-4-hydrogen`) | ~150–400ms, 0–low \$ | `colorability_score >= 0.5`; exits 0 if docket empty |
| `admit` | Cascade Spine | `Discriminator` / `Generator` (`google:gemini-decision-1` / `typesafe:jev-latest` / `google:gemini-4-hydrogen`) | ~200–600ms, 0–low \$ | `admissibility_score >= 0.5`; exits 0 if no exhibits |
| `vet` | Cascade Spine | `Discriminator` / `Generator` (`google:gemini-decision-1` / `typesafe:jev-latest` / `google:gemini-4-hydrogen`) | ~150–400ms, 0–low \$ | `vetting_score >= 0.8` $\implies$ vetted (0), else contested (1) |
| `audit` | Cascade Spine | `Generator` (`google:gemini-4-carbon`) | ~1.5–2.5s, contested only | `compliance_score >= 0.5` $\implies$ pass (0), else fail (1) |

---

## 6. The Token & Latency Sieve

The Caseload Pipeline functions as an aggressive filter funnel, eliminating the overwhelming majority of candidate pairs before invoking deep reasoning:

```mermaid
flowchart TD
    C100["100 Canons in Repository"] --> G2{"discover Gate<br/>(exists & triggers matched?)"}
    G2 -- "0 Matches" --> Exit0["Exit 0<br/>(0 tokens · ~8ms · Zero Credentials)"]
    G2 -- "Triggers Matched" --> C20["20 Candidate Canons"]
    
    C20 --> G3{"validate Gate<br/>(Malformed AST/schema?)"}
    G3 -- "Syntax Errors" --> Exit1["Exit 1 (Fail Fast · 0 tokens)"]
    G3 -- "Valid" --> V20["20 Validated Canons"]
    
    V20 --> S4["configure<br/>(Lazy JIT Evaluation: Missing Key? Exit 2)"]
    S4 --> G5{"docket Gate<br/>(Colorability ≥ 0.5)"}
    
    G5 -- "Dismissed (< 0.5)" --> SievePrune1["Pruned"]
    G5 -- "Docketed (≥ 0.5)" --> C3["3 Active Cases<br/>(~200ms · google:gemini-decision-1)"]
    
    C3 --> G6{"admit Gate<br/>(Admissibility ≥ 0.5)"}
    G6 -- "Inadmissible (< 0.5)" --> SievePrune2["Pruned"]
    G6 -- "Admitted (≥ 0.5)" --> E5["3 Active Cases · 5 Admitted Exhibits<br/>(~250ms · Discriminator)"]
    
    E5 --> G6_Vet{"vet Gate<br/>(Prima Facie ≥ 0.8 · Discriminator on vet)"}
    G6_Vet -- "Vetted (≥ 0.8)" --> VettedPass["2 Vetted Cases<br/>(0 output tokens · Folded into Verdict)"]
    G6_Vet -- "Contested (< 0.8)" --> G7["audit Adjudication<br/>(1 Contested Trial · google:gemini-4-carbon)"]
    VettedPass --> FinalVerdict["Final Verdict & Decree<br/>(~1.8s · Targeted Reasoning)"]
    G7 --> FinalVerdict
```

| Pipeline Step | Canons / Cases in Flight | Target Exhibits | Model Capability | Token Cost | Latency |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Initial Workspace** | 100 canons | All repo files | Local AST / Globs | 0 tokens | ~25ms |
| **`discover` Gate** | 20 candidates | 6 touched files | Local Regex / Globs | 0 tokens | ~8ms |
| **`docket` Gate** | 3 active Cases | 6 touched files | `Discriminator` / `Generator` (`google:gemini-decision-1` / `typesafe:jev-latest` / `google:gemini-4-hydrogen`) | 0 output tokens (~1,200 input) | ~200ms |
| **`admit` Gate** | 3 active Cases | 5 admitted hunks | `Discriminator` / `Generator` (`google:gemini-decision-1` / `typesafe:jev-latest` / `google:gemini-4-hydrogen`) | 0 output tokens (~1,800 input) | ~250ms |
| **`vet` Gate** | 2 vetted, 1 contested | 5 admitted hunks | `Discriminator` (`google:gemini-decision-1` / `typesafe:jev-latest`) | 0 output tokens | ~200ms |
| **`audit` Adjudication** | 1 contested trial | 2 exhibits | `Generator` (`google:gemini-4-carbon`) | ~1,500 tokens | ~1,200ms |
| **Total Funnel** | **3 evaluated** | **5 exhibits** | **Cascade Sieve** | **~4,500 tokens** | **< 2.0s** |

*(Versus naive evaluation: 100 canons × 6 files = 600 pairs $\approx$ 220,000 tokens and 45s latency. **~97% token reduction**).*

---

## 7. The Cumulative `Caseload` State Container

The `Caseload` is the central, immutable data envelope flowing through the pipeline. Rather than passing disjoint arguments between commands, each stage reads accumulated upstream state and enriches its own dedicated namespace on the shared `Caseload` object.

The formal Caseload domain models and granular payload schemas are specified in each node's architectural document.

### Top-Level Envelope Constraints

1. **Schema Versioning (`version`):** Every serialized `Caseload` envelope carries a top-level `version` identifier (currently `"1.0"`) establishing wire-format compatibility across piped CLI invocations and archived CI artifacts.
2. **Stage Namespace Isolation:** Each pipeline stage owns a dedicated, non-overlapping top-level optional namespace on the `Caseload` envelope (`.intake`, `.discovery`, `.validation`, `.config`, `.probe`, `.apprisal`, `.docket`, `.evidence`, `.vetting`, `.verdict`). Unexecuted or pruned stages are omitted from serialized JSON rather than emitted as null placeholders.
3. **Additive Immutability:** Stages enrich only their designated namespace while preserving pre-existing upstream namespaces bit-for-bit (and when chaining partial closures via `--caseload`, `configure` additively resolves unpopulated downstream node bindings onto `.config.nodes` while preserving un-overridden upstream node bindings).
4. **Three-Tier Determinism & Stability Contract:**
   - **Engine & Wire Determinism (Bit-for-Bit):** Heuristic node collections serialize in canonical DAG stage order (`apprise`, `docket`, `admit`, `vet`, `audit`), while all canon/file map keys, entity collections, per-case adjudications, and line annotations across stage payloads serialize in canonical lexicographical order so identical model responses, `--caseload` fast-forwards, and concurrent fold completion orders produce byte-for-byte identical JSON envelopes.
   - **Gate Verdict Stability (1D Flipper Contract):** Across repeated live runs at `temperature: 0.0`, heuristic stages anchor numeric scores to polarized bands away from their decision boundaries (`0.5` for `apprise`, `docket`, `admit`, `audit`; `0.8` for `vet`) so per-canon gate classifications (`applicable`, `docketed`, `admitted`, `vetted`/`contested`, `pass`/`fail`) remain unanimous even when extended reasoning trajectories vary.
   - **Guidance & Span Granularity (Benign Generative Variance):** Natural wording variations in rationale/remediation prose and minor line-span granularity shifts under extended reasoning are tolerated and never constitute verdict flippers, bounded by deterministic structural anchoring guardrails.

### Stage Payloads & Authoritative Specifications

Each pipeline stage owns a dedicated, non-overlapping field on the cumulative `Caseload`:

| Field on `Caseload` | Enriched By | Track | Payload Summary | Authoritative Specification |
| :--- | :--- | :--- | :--- | :--- |
| `.intake` | `intake` | Branch A (Filing) | Tendered exhibits: diff hunks, target paths, PR metadata, or intent queries | [`nodes/intake.md`](nodes/intake.md#caseload-delta) |
| `.discovery` | `discover` | Branch A (Filing) | Candidate canons matching path triggers & target file intersections | [`nodes/discover.md`](nodes/discover.md#caseload-delta) |
| `.validation` | `validate` | Branch A (Filing) | Deterministic AST linting and frontmatter schema validation results | [`nodes/validate.md`](nodes/validate.md#caseload-delta) |
| `.config` | `configure` | Branch B (Env) | Resolved default `provider` slug and per-node `<provider>:<model>` + optional thinking bindings | [`nodes/configure.md`](nodes/configure.md#caseload-delta) |
| `.probe` | `probe` | Diagnostic Leaf | Deduplicated target endpoint reachability, bound nodes, roundtrip latency (ms), and resolved models | [`nodes/probe.md`](nodes/probe.md#caseload-delta) |
| `.apprisal` | `apprise` | Apprisal Track | Prospective statutory jurisdiction assessments (`apprisal_score`, `apprisal_summary`) | [`nodes/apprise.md`](nodes/apprise.md#caseload-delta) |
| `.docket` | `docket` | Dispute Spine | Macro triage colorability assessments and list of opened active cases | [`nodes/docket.md`](nodes/docket.md#caseload-delta) |
| `.evidence` | `admit` | Dispute Spine | Micro triage evidence admissibility: admitted file exhibits per active case | [`nodes/admit.md`](nodes/admit.md#caseload-delta) |
| `.vetting` | `vet` | Dispute Spine | Prima facie compliance vetting assessments (`vetted_cases`, `contested_cases`) | [`nodes/vet.md`](nodes/vet.md#caseload-delta) |
| `.verdict` | `audit` | Dispute Spine | Final substantive adjudications, compliance scores, decrees, and line annotations | [`nodes/audit.md`](nodes/audit.md#caseload-delta) |

Downstream subcommands fast-forward across any stage whose corresponding namespace is already populated on an incoming `Caseload` (via `--caseload <path|->`).

---

## 8. Observability & The Telemetry Event Stream

Canon Clerk strictly separates **substantive evaluation records** from **operational execution telemetry**:

- **Substantive Record (`Caseload`):** Represents solely substantive findings, omitting wall-clock timestamps, token counters, and intermediate thought traces so serialized `caseload.json` envelopes remain free of operational telemetry churn across git diffs, cached artifacts, and hermetic snapshot tests.
- **Event Logging (`stderr` / `--log-file`):** Operational metrics (wall-clock milliseconds, per-call and stage-summary token usage, incremental work-unit completion, rate-limit cooldown and transient retry waits, time-to-first-token/thought) and streaming intermediate chunks (thought deltas) are emitted out-of-band via a structured **Telemetry Event Stream** comprising seven lifecycle event categories:
  - **Stage Start (`StageStart`):** Signals entry into a `PipelineStage` along with an ISO-8601 `timestamp`.
  - **Work-Unit Progress (`StageProgress`):** Reports incremental completion counts (`completed` of `total` batches, cases, or trials alongside a concise unit `label`) as concurrent or sequential model calls finish within multi-unit heuristic stages (`docket`, `admit`, `vet`, `audit`).
  - **Reasoning Delta (`Thought`):** Streams incremental model reasoning `delta` chunks associated with an active `PipelineStage`.
  - **Per-Call Model Telemetry (`ModelCall`):** Emitted immediately as each individual generative or discriminative model call completes within an active `PipelineStage`, recording `resolved_model`, `latency_ms`, optional unit `label` (such as `canon_path` or slice identifier), and per-call `usage` (`prompt_tokens`, `completion_tokens`, `reasoning_tokens`, `total_tokens`)—ensuring per-canon thinking-token outliers and completed call metrics are flushed in real time even if a sibling call later fails or times out.
  - **Provider Retry & Cooldown Wait (`ProviderRetry`):** Emitted immediately *before* a heuristic stage worker sleeps on a coordinated per-origin rate-limit cooldown or transient retry backoff ([`transport.md` §6](transport.md#6-out-of-band-telemetry--cli-transport-controls)), recording the active `stage`, optional unit `label`, target origin `host`, retry `attempt` and `max_retries` bounds, planned `wait_ms`, and concise `reason` (such as `HTTP 429` rate limit, `HTTP 503` overload, or wire timeout).
  - **Stage Completion (`StageFinish`):** Records stage completion with elapsed wall-clock `duration_ms` and optional aggregated token `usage` metrics.
  - **Pipeline Completion (`PipelineFinish`):** Summarizes cumulative `total_duration_ms` and aggregate `total_tokens` across the completed DAG closure.
- **Executed-Stage Scoping:** Lifecycle events are emitted exclusively for pipeline stages that execute during the active invocation. Ancestor stages fast-forwarded from an incoming `--caseload` and downstream stages bypassed by deterministic short-circuit gates are omitted from the stream, and `PipelineFinish` aggregates `total_tokens` strictly across executed stages.
- **Structured Log Sink, Crate-Scoped Verbose Filtering & Non-Interactive Hygiene:** Supplying `--log-file <path>` (or the `CANON_CLERK_LOG_FILE` environment variable) persists the structured event stream as newline-delimited JSON Lines (`JSONL`) on disk independently of terminal streams. Interactive TTY runs immediately announce entry into remote/heuristic stages (such as `--caseload` handoffs into `audit`), multi-unit completion ticks, and multi-second rate-limit cooldown waits (even without `-v`, so rolling TPM window pauses never appear frozen) on `stderr`, while `--verbose` (`-v` / `CANON_CLERK_VERBOSE`) surfaces full stage-entry, per-unit `k/N` completion, every transient retry/cooldown wait, and compact thought diagnostics while scoping default debug tracing strictly to Canon Clerk's own crate targets unless overridden via `RUST_LOG` (suppressing third-party HTTP/TLS socket noise). Non-TTY pipes, `CI`, `NO_COLOR`, and `--quiet` predicate environments automatically suppress interactive decorations and ANSI escape sequences so `stdout` payloads remain uncorrupted.
