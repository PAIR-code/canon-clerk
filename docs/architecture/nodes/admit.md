# Stage 6: Docket Evidence (`admit`)

**Status:** Authoritative Architectural Standard  
**Subcommand:** `canon-clerk admit`  
**Aliases:** `docket-evidence`, `docket-targets`  
**Pipeline Track:** Adjudication Spine (Micro Triage)

---

## 1. Domain Concept & Role

`admit` performs **Micro Triage: Evidentiary Relevance Screening** for each active Case on the docket. In the court clerkship taxonomy, it represents the clerk reviewing tendered documents, affidavits, and physical exhibits to formally admit only relevant evidence into the case record prior to trial.

- **Imperative Verb:** `admit`
- **Court Clerkship Role:** Micro triage establishing evidentiary admissibility.
- **Metric Pair:** `admissibilityScore` (number [0.0, 1.0]) and `admissibilitySummary` (string rationale).
- **Core Question:** *"For an active Case, is this specific file/diff hunk admissible as relevant evidence?"*

---

## 2. Dependencies

- **Direct Prerequisites:** `docket` (requires active cases on `activeDocket`).
- **Transitive Prerequisites:** `intake`, `discover`, `validate`, `configure`.
- **Pruned from Execution:** `probe`, Stage 7 (`audit`).

---

## 3. Specific Inputs

### Standard Streams & CLI Options
- Incoming `Caseload` via `--caseload <path|->` (containing `activeDocket`, `diffs`, `config`).
- `--threshold <number>`: Admissibility relevance threshold (default: `0.5`).
- `--json`: Emits enriched Caseload containing the `.evidence` block.

### Context Supplied per Case
- Governing canon statute: invariant, full text, and requested `inspect` fields.
- Target candidates: individual modified files, unified diff hunks, and resolved `references:` files.

---

## 4. Process & Logic

1. **Per-Case Evidentiary Review:**  
   Iterates through each canon listed on `caseload.docket.activeDocket`.
2. **Fast Heuristic Screening (`gemini-3.5-flash-lite`):**  
   Evaluates each candidate file / diff hunk / reference document against the specific canon's requirements.
3. **Constrained Decoding Schema (Domain-Indirected, Reason-First):**
   ```json
   {
     "exhibits": [
       {
         "filePath": "packages/cli/src/commands/docket.ts",
         "admissibilitySummary": "Contains option definitions for new CLI command.",
         "admissibilityScore": 0.95
       }
     ]
   }
   ```
   Generating `admissibilitySummary` before `admissibilityScore` provides a chain-of-thought scratchpad, anchoring reproducible probability distributions.
4. **Admissibility Threshold (`admissibilityScore >= 0.5`):**
   - Exhibits scoring $\ge 0.5$ are admitted into evidence for that Case.
   - Irrelevant diff hunks (e.g. documentation edits or unrelated refactors) are excluded (`admissibilityScore < 0.5`).
5. **Dismissal of Cases with Zero Exhibits:** If an active Case retains zero admitted exhibits, it is dismissed without trial.

---

## 5. Outputs & Caseload Delta

Populates the `.evidence` field on the cumulative `Caseload`:

```ts
export interface AdmittedExhibit {
  /** Repository-relative path to admitted file or exhibit */
  readonly filePath: string;

  /** Relevance score of exhibit to governing canon [0.0, 1.0] */
  readonly admissibilityScore: number;

  /** Rationale for admitting exhibit into evidence */
  readonly admissibilitySummary: string;
}

export interface CanonEvidenceExhibits {
  /** Canon file path governing these exhibits */
  readonly canonPath: string;

  /** Admitted evidence exhibits */
  readonly exhibits: readonly AdmittedExhibit[];
}

export interface CaseloadEvidence {
  /** Admitted exhibits keyed by canon path */
  readonly exhibits: Record<string, CanonEvidenceExhibits>;
}
```

### Caseload Delta
- `caseload.evidence`: Attached with the map of admitted `exhibits` for each active canon.

---

## 6. Gate & Error Semantics

- **The Zero-Evidence Short-Circuit (Exit Code 0):**  
  **If all active cases retain zero admitted exhibits, execution terminates immediately with exit code `0`.**  
  *Prevents scheduling reasoning models when no relevant evidence exists in the change.*
- **Exit Code 0 (Exhibits Admitted):** Proceeds downstream to `audit` or emits evidence Caseload.
- **Exit Code 2 (Provider / Parse Error):** Provider API error or schema violation.
