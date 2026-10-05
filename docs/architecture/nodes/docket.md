# Stage 5: Docket Canons (`docket`)

**Status:** Authoritative Architectural Standard  
**Subcommand:** `canon-clerk docket`  
**Aliases:** `docket-canons`, `color`  
**Pipeline Track:** Adjudication Spine (Convergence Point)

---

## 1. Domain Concept & Role

`docket` performs **Macro Triage: Subject-Matter Jurisdiction Screening** across all candidate canons. In the court clerkship taxonomy, it represents the clerk reviewing petitions against the court's subject-matter jurisdiction to determine whether a claim is legally colorable before opening a formal case file on the active docket.

- **Imperative Verb:** `docket`
- **Court Clerkship Role:** Macro triage establishing subject-matter jurisdiction.
- **Metric Pair:** `colorabilityScore` (number [0.0, 1.0]) and `colorabilitySummary` (string rationale).
- **Core Question:** *"Does this candidate canon have a colorable claim of jurisdiction over this PR as a whole?"*

---

## 2. Dependencies

- **Direct Prerequisites:**
  - `validate` (Branch A: validated candidate canons).
  - `configure` (Branch B: resolved provider credentials and model specifiers).
- **Transitive Prerequisites:** `intake`, `discover`.
- **Pruned from Execution:** `probe`, Stages 6–7.

---

## 3. Specific Inputs

### Standard Streams & CLI Options
- Incoming `Caseload` via `--caseload <path|->` (containing `intake`, `discovery`, `validation`, and `config`).
- `--threshold <number>`: Jurisdiction screening threshold (default: `0.5`).
- `--json`: Emits enriched Caseload containing the `.docket` block.

### Context Supplied to Screener
- High-level PR context: `pr_title`, `pr_body`.
- Diff summary metrics: files changed, lines added/deleted, and touched file paths.
- Candidate canon summaries: `id`, `title`, and invariant statement.

---

## 4. Process & Logic

1. **Aggregate Single-Turn Screening:**  
   Unlike trial evaluation which evaluates cases independently, `docket` screens **all candidate canons in a single aggregate prompt turn** using `gemini-3.5-flash-lite`.
2. **Constrained Grammar Decoding:**  
   Forces structured JSON output with guaranteed schema keys:
   ```json
   {
     "cases": {
       ".canons/cli/cli-flags-kebab-case.md": {
         "colorabilityScore": 0.95,
         "colorabilitySummary": "PR introduces new command flags in packages/cli.",
         "status": "docketed"
       }
     }
   }
   ```
3. **Threshold Gate (`colorabilityScore >= 0.5`):**
   - Canons scoring $\ge 0.5$ establish jurisdiction and are entered into `activeDocket`.
   - Canons scoring $< 0.5$ are marked `dismissed` with summary rationale.
4. **Latency & Token Economy:** Executes in ~400ms, expending only ~1,200 tokens across 20+ candidate canons.

---

## 5. Outputs & Caseload Delta

Populates the `.docket` field on the cumulative `Caseload`:

```ts
export interface ColorabilityAssessment {
  /** Numerical score indicating colorable subject-matter jurisdiction [0.0, 1.0] */
  readonly colorabilityScore: number;

  /** Reasoning justifying whether jurisdiction applies to the PR context */
  readonly colorabilitySummary: string;

  /** Status outcome */
  readonly status: 'docketed' | 'dismissed';
}

export interface CaseloadDocket {
  /** Colorability assessments keyed by canon path */
  readonly cases: Record<string, ColorabilityAssessment>;

  /** List of canon paths admitted onto the Active Docket */
  readonly activeDocket: readonly string[];

  /** Optional diagnostic anomalies manifest */
  readonly anomalies?: DocketAnomaliesManifest | undefined;
}
```

### Caseload Delta
- `caseload.docket`: Attached with jurisdiction assessments for all candidate canons and the `activeDocket` list of active Cases.

---

## 6. Gate & Error Semantics

- **The Empty Docket Short-Circuit (Exit Code 0):**  
  **If zero candidate canons achieve `colorabilityScore >= 0.5`, the run terminates immediately with exit code `0`.**  
  *No substantive trials (`audit`) are scheduled, avoiding hundreds of thousands of deep-reasoning tokens.*
- **Exit Code 0 (Active Cases Present):** When one or more cases are docketed, proceeds downstream to `admit` or emits docket Caseload.
- **Exit Code 2 (Provider / Parse Error):** Network connectivity failure or malformed provider response.
