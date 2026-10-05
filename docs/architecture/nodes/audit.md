# Stage 7: Judicial Adjudication (`audit`)

**Status:** Authoritative Architectural Standard  
**Subcommand:** `canon-clerk audit`  
**Pipeline Track:** Adjudication Spine (The Judicial Terminus)

---

## 1. Domain Concept & Role

`audit` performs **Substantive Adjudication: Merits Evaluation & Verdict Generation** for all active cases. In the court clerkship taxonomy, it represents the Judge taking the bench, hearing arguments on the admitted exhibits, evaluating affirmative defenses (`Exception`), and rendering formal decrees with actionable remediation.

- **Imperative Verb:** `audit`
- **Court Clerkship Role:** Judicial trial and decree rendering.
- **Metric Pair:** `complianceScore` (number [0.0, 1.0]) and `complianceSummary` (string decree).
- **Core Question:** *"Given the admitted exhibits and governing invariant/exceptions, does the evidence comply with canon statute?"*

---

## 2. Dependencies

- **Direct Prerequisites:** `admit` (requires admitted exhibits in `caseload.evidence`).
- **Transitive Prerequisites:** `intake`, `discover`, `validate`, `configure`, `docket`.
- **Pruned from Execution:** `probe` (never scheduled during review cascades).

---

## 3. Specific Inputs

### Standard Streams & CLI Options
- Incoming `Caseload` via `--caseload <path|->` (or backfilled in-memory across the full DAG).
- Positional target paths / globs, positional `-`, or `--diff <path|->` (when executing in telescoping backfill mode).
- `--json`: Emits the final `Caseload` record including `.verdict`.

### Evidentiary Context Supplied per Trial
- Canon rule text: Invariant (What), `Exception` (When), `Rationale` (Why), and `Remediation` (How).
- Admitted exhibits: Only files, diff hunks, and reference documents admitted during `admit`.

---

## 4. Process & Logic

1. **One Trial per Case per Prompt Turn:**  
   Each active case is evaluated in an **independent, isolated trial**:
   - **Isolation:** Prevents cross-canon hallucination; Canon A's exceptions never bleed into Canon B's evaluation.
   - **Bounded Token Footprint:** Prompts only contain the governing canon and its admitted exhibits.
   - **Concurrency:** Independent trials execute concurrently across model calls using `Promise.all`.
2. **Frontier Reasoning Model Tier (`gemini-3.8-pro`):**  
   Evaluates substantive compliance with extended thinking/reasoning enabled.
3. **The Four-Step Judicial Decision Tree:**
   - **Step 1 (Invariant Evaluation):** Evaluates admitted exhibits against the normative invariant (What). If compliant $\implies$ `complianceScore = 1.0`, status `pass`.
   - **Step 2 (Exception Screening):** If a violation is found, evaluates declared `Exception` clauses. If an exception's criteria are semantically satisfied $\implies$ short-circuits to conditional `pass` (`complianceScore >= 0.5`), documenting the matched exception.
   - **Step 3 (Remediation Formulation):** If no exception applies $\implies$ violation stands (`complianceScore < 0.5`, status `fail`), and formulates actionable contributor remediation (How).
   - **Step 4 (Line Annotations):** Emits precise file, line, and column coordinates for each violation.

---

## 5. Outputs & Caseload Delta

Populates the `.verdict` field on the cumulative `Caseload`:

```ts
export interface CodeAnnotation {
  readonly path: string;
  readonly startLine: number;
  readonly endLine: number;
  readonly startColumn?: number | undefined;
  readonly endColumn?: number | undefined;
  readonly annotationLevel: 'failure' | 'warning' | 'notice';
  readonly message: string;
  readonly title?: string | undefined;
}

export interface CanonAdjudication {
  /** Canon file path evaluated */
  readonly canonPath: string;

  /** Compliance score indicating statute adherence [0.0, 1.0] */
  readonly complianceScore: number;

  /** Substantive decree explaining compliance or violation */
  readonly complianceSummary: string;

  /** Verdict status */
  readonly status: 'pass' | 'fail';

  /** Line-level code annotations */
  readonly annotations: readonly CodeAnnotation[];
}

export interface CaseloadVerdict {
  /** Overall review gate outcome */
  readonly status: 'pass' | 'fail';

  /** High-level verdict summary */
  readonly summary: string;

  /** Substantive adjudications per active case */
  readonly adjudications: readonly CanonAdjudication[];
}
```

### Caseload Delta
- `caseload.verdict`: Attached with high-level `status`, summary decree, and individual case adjudications.

---

## 6. Gate & Error Semantics

- **Review Gate Passing (Exit Code 0):**  
  **If all evaluated cases pass (`status: 'pass'`, `complianceScore >= 0.5`), the process exits with status `0`.**  
  *All architectural invariants and permissible exceptions are satisfied.*
- **Review Gate Failing (Exit Code 1):**  
  **If any evaluated case violates an invariant (`status: 'fail'`, `complianceScore < 0.5`), the process exits with status `1`.**  
  *Blocks PR merge in CI and emits actionable line annotations and remediation advice.*
- **Exit Code 2 (Fatal / Execution Error):** Unresolvable configuration, fatal provider API failure, or process abort.
