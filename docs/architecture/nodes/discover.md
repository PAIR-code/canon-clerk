# Stage 2: Candidate Identification (`discover`)

**Status:** Authoritative Architectural Standard  
**Subcommand:** `canon-clerk discover`  
**Aliases:** `check-triggers`  
**Pipeline Track:** Branch A (The Filing Track)

---

## 1. Domain Concept & Role

`discover` acts as the **Jurisdictional Path Filter** in the evaluation pipeline. In the court clerkship taxonomy, it represents the clerk checking the initial jurisdictional caption of a filing against the court's rules to identify which statutory rule packs (canons) could possibly apply to the files touched.

- **Imperative Verb:** `discover`
- **Court Clerkship Role:** Candidate canon identification via path trigger matching.
- **Metric Pair:** N/A (Deterministic glob intersection).

---

## 2. Dependencies

- **Direct Prerequisites:** `intake` (requires parsed target paths or diffs).
- **Transitive Prerequisites:** None.
- **Pruned from Execution:** Branch B (`configure`), `probe`, Stages 3–7.

---

## 3. Specific Inputs

### Standard Streams & CLI Options
- Incoming `Caseload` via `--caseload <path|->` (or backfilled in-memory via `intake`).
- `--canon <path|glob>`: Explicitly filter discovery to specific canon files or subsets.
- `-q, --quiet`: Predicate mode; suppresses stdout output and uses exit codes for presence of matches.
- `--json`: Emits the updated `Caseload` containing the `.discovery` block.

---

## 4. Process & Logic

1. **Target Path Extraction:** Extracts all repository-relative file paths from `caseload.intake.diffs` (or positional target arguments).
2. **Canon Corpus Enumeration:** Discovers all candidate canons across the workspace (default pattern: `**/.canons/**/*.md`).
3. **Monorepo Scope Inheritance:** Applies implicit directory scoping:
   - Global canons in `.canons/**` apply repository-wide.
   - Scoped canons located in `<scope>/.canons/**` automatically inherit an implicit `<scope>/**` trigger boundary. If zero modified target files reside in `<scope>/`, the canon is excluded from candidates.
4. **Trigger Glob Matching:** Evaluates the declared `triggers:` globs in each canon against the list of modified target paths using picomatch / minimatch semantics.
5. **Join Calculation:** Computes `triggersJoin` mapping each candidate canon to the specific modified files that activated it.

---

## 5. Outputs & Caseload Delta

Populates the `.discovery` field on the cumulative `Caseload`:

```ts
export interface CaseloadDiscovery {
  /** Modified target file paths evaluated */
  readonly targetFiles: readonly string[];

  /** Discovered candidate canon paths matching targets */
  readonly candidateCanons: readonly string[];

  /** Map of canon paths to matched target file paths */
  readonly triggersJoin: Record<string, readonly string[]>;
}
```

### Caseload Delta
- `caseload.discovery`: Attached with `targetFiles`, `candidateCanons`, and `triggersJoin`.

---

## 6. Gate & Error Semantics

- **The Zero-Candidate Short-Circuit (Exit Code 0):**  
  **If zero candidate canons match the modified files, the run terminates immediately with exit code `0`.**  
  *Un-governed PRs (e.g. updating documentation, dependencies, or unmonitored code) exit in ~8ms with 0 tokens, 0 network calls, and zero provider credentials required.*
- **Exit Code 0 (Matches Found):** When candidate canons are matched, proceeds downstream or emits discovery Caseload.
- **Exit Code 1 (Predicate Mode `-q`):** In `--quiet` predicate mode, exits `1` if zero candidate canons matched (useful for shell scripting and CI conditionals).
- **Exit Code 2 (Usage / Error):** Invalid glob patterns or inaccessible workspace directories.
