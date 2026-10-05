# Stage 2: Candidate Identification (`discover`)

**Status:** Authoritative Architectural Standard  
**Stage:** 2  
**Core Domain Engine:** `@canon-clerk/core`  
**Driving Adapters:** `@canon-clerk/cli` (`discover`, `check-triggers`), `@canon-clerk/action`, `@canon-clerk/integration-tests-private`

---

## 1. Domain Concept & Role (`core`)

`discover` acts as the **Jurisdictional Path Filter** in the evaluation pipeline. In the court clerkship taxonomy, it represents the clerk checking the initial jurisdictional caption of a filing against the court's rules to identify which statutory rule packs (canons) could possibly apply to the files touched.

- **Imperative Verb:** `discover`
- **Court Clerkship Role:** Candidate canon identification via path trigger matching.
- **Metric Pair:** N/A (Deterministic glob intersection).

---

## 2. Dependencies & Prerequisites (`core`)

- **Direct Prerequisites:** `intake` (requires parsed target paths or diffs in `caseload.intake`).
- **Transitive Prerequisites:** None.
- **Incoming Caseload:** Requires `caseload.intake` to be present.

---

## 3. Core Functional Contract (`packages/core`)

```ts
export interface DiscoverOptions {
  readonly workspaceRoot: string;
  readonly canonGlobs?: readonly string[] | undefined;
  readonly explicitCanonFilter?: readonly string[] | undefined;
}

export function executeDiscover(
  options: DiscoverOptions,
  caseload: Caseload
): Promise<Caseload>;
```

### Caseload Delta
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

### Domain Short-Circuit Invariant
If `candidateCanons.length === 0`:
- The core engine sets `candidateCanons: []` and returns the enriched Caseload.
- Downstream execution ceases immediately, returning a clean success state without evaluating configuration or spending tokens.

---

## 4. Process & Domain Logic (`core`)

1. **Target Path Extraction:** Extracts all repository-relative file paths from `caseload.intake.diffs` (or target paths).
2. **Canon Corpus Enumeration:** Discovers all candidate canons across the workspace (default pattern: `**/.canons/**/*.md`).
3. **Monorepo Scope Inheritance:** Applies implicit directory scoping:
   - Global canons in `.canons/**` apply repository-wide.
   - Scoped canons located in `<scope>/.canons/**` automatically inherit an implicit `<scope>/**` trigger boundary. If zero modified target files reside in `<scope>/`, the canon is excluded from candidates.
4. **Trigger Glob Matching:** Evaluates the declared `triggers:` globs in each canon against the list of modified target paths using picomatch / minimatch semantics.
5. **Join Calculation:** Computes `triggersJoin` mapping each candidate canon to the specific modified files that activated it.

---

## 5. Driving Adapter: CLI (`packages/cli`)

The CLI exposes `discover` (aliased as `check-triggers`) as an imperative subcommand:

```bash
# Evaluate discovery on in-flight diff stream:
git diff origin/main | canon-clerk discover --diff -

# Evaluate discovery against an upstream Caseload:
canon-clerk discover --caseload caseload-1.json --json

# Predicate mode (-q):
git diff origin/main | canon-clerk check-triggers -q -
```

### CLI Flags & Options
- `-q, --quiet`: Predicate mode. Suppresses stdout and indicates match presence via exit code.
- `--canon <path|glob>`: Explicitly restrict candidate canons to a specific subset.
- `--format <stylish|json|compact>`: Formats matched canons and triggering files.
- `--caseload <path|->`: Ingests upstream Caseload JSON.

### CLI Exit Codes
- **0:** Candidate canons matched and discovery attached (or clean execution).
- **0 (Short-Circuit):** Zero candidate canons matched; logs summary and exits immediately.
- **1 (Predicate Mode `-q`):** Exits 1 if zero candidate canons matched.
- **2:** Usage error or invalid glob syntax.

---

## 6. Driving Adapter: GitHub Action (`packages/action`)

1. **Automated Candidate Check:** Calls `executeDiscover` with the Caseload produced by `executeIntake`.
2. **Fast-Pass Evaluation:** If `candidateCanons.length === 0`, the action records a successful, neutral Check Run conclusion (`neutral` or `success`), logs that no governed files were touched, and terminates cleanly in <2 seconds without requiring `GEMINI_API_KEY`.
3. **Step Summary:** Emits a Markdown table of matched canons and triggering files into `GITHUB_STEP_SUMMARY`.

---

## 7. Driving Adapter: Integration Tests (`packages/integration-tests-private`)

Integration tests invoke `executeDiscover` directly against simulated monorepo directory layouts, asserting that scope inheritance (`packages/cli/.canons/` $\implies$ `packages/cli/**`) and complex glob patterns match accurately across OS platforms.
