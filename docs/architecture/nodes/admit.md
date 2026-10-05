# Evidence Admissibility Triage (`admit`)

**Status:** Authoritative Architectural Standard  
**Core Domain Engine:** `@canon-clerk/core`  
**Driving Adapters:** `@canon-clerk/cli` (`admit`, `docket-evidence`, `docket-targets`), `@canon-clerk/action`, `@canon-clerk/integration-tests-private`

---

## 1. Domain Concept & Role (`core`)

`admit` performs **Micro Triage: Evidentiary Relevance Screening** for each active Case on the docket. In the court clerkship taxonomy, it represents the clerk reviewing tendered documents, affidavits, and physical exhibits to formally admit only relevant evidence into the case record prior to trial.

- **Imperative Verb:** `admit`
- **Court Clerkship Role:** Micro triage establishing evidentiary admissibility.
- **Metric Pair:** `admissibilityScore` (number [0.0, 1.0]) and `admissibilitySummary` (string rationale).
- **Core Question:** *"For an active Case, is this specific file/diff hunk admissible as relevant evidence?"*

---

## 2. Dependencies & Prerequisites (`core`)

- **Direct Prerequisites:** `docket` (requires active cases in `caseload.docket.activeDocket`).
- **Transitive Prerequisites:** `intake`, `discover`, `validate`, `configure`.
- **Pruned from Execution:** `probe`, `audit`.

---

## 3. Core Functional Contract (`packages/core`)

```ts
export interface AdmitOptions {
  readonly threshold?: number | undefined; // default: 0.5
  readonly screenerModel?: string | undefined;
}

export function executeAdmit(
  options: AdmitOptions,
  caseload: Caseload
): Promise<Caseload>;
```

### Caseload Delta
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

### Domain Short-Circuit Invariant
If all active cases retain zero admitted exhibits:
- Execution terminates immediately with exit code `0`.
- Zero substantive trials (`audit`) are scheduled, avoiding reasoning token expenditures.

---

## 4. Process & Domain Logic (`core`)

1. **Per-Case Evidentiary Review:** Iterates through each canon on `caseload.docket.activeDocket`.
2. **Fast Heuristic Screening (`gemini-3.5-flash-lite`):** Evaluates candidate target files, diff hunks, and reference documents against the canon's specific requirements.
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
   - Irrelevant diff hunks are excluded (`admissibilityScore < 0.5`).
5. **Dismissal of Cases with Zero Exhibits:** If an active Case retains zero admitted exhibits, it is dismissed without trial.

---

## 5. Driving Adapter: CLI (`packages/cli`)

The CLI exposes `admit` (aliased as `docket-evidence` and `docket-targets`):

```bash
# Execute evidence triage against an upstream Caseload:
canon-clerk admit --caseload caseload-4.json --json

# Run telescoping pipeline stopping at admit:
git diff origin/main | canon-clerk admit --diff -
```

### CLI Flags & Environment
- `--threshold <number>`: Admissibility threshold (default: `0.5`).
- `--caseload <path|->`: Ingests upstream Caseload.
- `--json`: Emits enriched Caseload JSON.

### CLI Exit Codes
- **0:** Exhibits admitted and attached (or zero-evidence short-circuit).
- **2:** Provider connection error or model response failure.

---

## 6. Driving Adapter: GitHub Action (`packages/action`)

1. **Evidence Screening Step:** Invokes `executeAdmit` with the `Caseload`.
2. **Exhibit Accounting:** Logs admitted diff hunks and persistent references per case.
3. **Early Exit:** If zero cases retain admitted evidence, concludes the Check Run as passing without scheduling reasoning models.

---

## 7. Driving Adapter: Integration Tests (`packages/integration-tests-private`)

Integration tests invoke `executeAdmit` across multi-file PR fixtures, verifying that peripheral changes (e.g. docs, lockfiles) are cleanly filtered out from active cases governing code conventions.
