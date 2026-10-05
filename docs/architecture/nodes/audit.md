# Stage 7: Judicial Adjudication (`audit`)

**Status:** Authoritative Architectural Standard  
**Stage:** 7  
**Core Domain Engine:** `@canon-clerk/core`  
**Driving Adapters:** `@canon-clerk/cli` (`audit`), `@canon-clerk/action`, `@canon-clerk/integration-tests-private`

---

## 1. Domain Concept & Role (`core`)

`audit` performs **Substantive Adjudication: Merits Evaluation & Verdict Generation** for all active cases. In the court clerkship taxonomy, it represents the Judge taking the bench, hearing arguments on the admitted exhibits, evaluating affirmative defenses (`Exception`), and rendering formal decrees with actionable remediation.

- **Imperative Verb:** `audit`
- **Court Clerkship Role:** Judicial trial and decree rendering.
- **Metric Pair:** `complianceScore` (number [0.0, 1.0]) and `complianceSummary` (string decree).
- **Core Question:** *"Given the admitted exhibits and governing invariant/exceptions, does the evidence comply with canon statute?"*

---

## 2. Dependencies & Prerequisites (`core`)

- **Direct Prerequisites:** `admit` (requires admitted exhibits in `caseload.evidence`).
- **Transitive Prerequisites:** `intake`, `discover`, `validate`, `configure`, `docket`.
- **Pruned from Execution:** `probe` (never scheduled during review cascades).

---

## 3. Core Functional Contract (`packages/core`)

```ts
export interface AuditOptions {
  readonly auditorModel?: string | undefined;
  readonly reasoningBudget?: number | undefined;
}

export function executeAudit(
  options: AuditOptions,
  caseload: Caseload
): Promise<Caseload>;
```

### Caseload Delta
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

---

## 4. Process & Domain Logic (`core`)

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

## 5. Driving Adapter: CLI (`packages/cli`)

The CLI exposes `audit` as its flagship evaluation command:

```bash
# Evaluate in-flight diff in telescoping mode:
git diff origin/main | canon-clerk audit --diff -

# Adjudicate an admitted Caseload from upstream pipeline:
canon-clerk audit --caseload caseload-5.json

# Output complete Caseload record to file:
git diff origin/main | canon-clerk audit --diff - --json > final-caseload.json
```

### CLI Output & Stream Formatting
- **TTY Progress:** Displays interactive spinners and step execution traces on `stderr`.
- **Verdict Report:** Emits formatted Markdown or stylish terminal summary to `stdout`.
- **Telemetry Log:** Optionally redirects event stream via `--log-file <path>`.

### CLI Exit Codes
- **0:** All evaluated cases pass (`status: 'pass'`, `complianceScore >= 0.5`).
- **1:** Architectural violation detected (`status: 'fail'`, `complianceScore < 0.5`).
- **2:** Fatal error, missing credentials, or provider failure.

---

## 6. Driving Adapter: GitHub Action (`packages/action`)

1. **Full DAG Invocation:** Drives the complete Caseload pipeline to `executeAudit`.
2. **GitHub Check Run Creation:**
   - Creates a GitHub Check Run (`octokit.rest.checks.create`).
   - Maps overall status to Check Run conclusion:
     - `status: 'pass'` $\implies$ `conclusion: 'success'` 🟢
     - `status: 'fail'` $\implies$ `conclusion: 'failure'` 🔴
3. **Line-Level GitHub Annotations:** Converts `adjudications[].annotations` into Check Run annotations (`path`, `start_line`, `end_line`, `annotation_level: 'failure'`, `message`), placing visual review flags directly on the PR files diff tab.
4. **Markdown Step Summary:** Writes an executive decree and per-case breakdown to `$GITHUB_STEP_SUMMARY`.

---

## 7. Driving Adapter: Integration Tests (`packages/integration-tests-private`)

Integration tests invoke `executeAudit` against complex diff scenarios with live Gemini reasoning models, verifying that:
- Legitimate `Exception` clauses short-circuit to `pass`.
- Invariant violations generate accurate line annotations and `Remediation` guidance.
- Decrees are reproducible and deterministic across test runs.
