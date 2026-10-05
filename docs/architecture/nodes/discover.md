# Candidate Identification (`discover`)

**Status:** Authoritative Architectural Standard  
**Core Domain Engine:** `@canon-clerk/core`  
**Driving Adapters:** `@canon-clerk/cli` (`discover`), `@canon-clerk/action`, `@canon-clerk/integration-tests-private`

---

## 1. Domain Concept & Role (`core`)

`discover` acts as the **Multi-Plane Relevance Sieve & Dual Pruning Gate** of the pipeline. In the court clerkship taxonomy, it represents the clerk reviewing tendered filings to resolve referenced files, verify state preconditions, enforce monorepo jurisdictional boundaries, and compare all tendered exhibits against the court's codified canons:

1. **Exhibit Materialization:** Resolves exhibit discovery directives (expanding directory recursion roots, matching globs against workspace checkout into concrete file exhibits).
2. **State Precondition Verification:** Evaluates `requires:` path patterns against the Target File Tree to ensure baseline environment dependencies exist before admitting candidate canons.
3. **Subordination & Scope Containment:** Enforces monorepo boundaries, ensuring scoped canons evaluate strictly within their enclosing `<scope>/`.
4. **Two-Sided Mutual Pruning:**
   - **Prunes Inapplicable Canons:** Canons with unmet `requires` preconditions or zero matching required exhibits are dropped $\implies$ `candidateCanons`.
   - **Prunes Un-inspected Exhibits:** Exhibits not inspected or triggered by any surviving candidate canon are dropped $\implies$ `activeExhibits` (preserving context-window hygiene).

- **Imperative Verb:** `discover`
- **Court Clerkship Role:** Exhibit materialization, precondition verification, scope containment, candidate canon identification, and mutual exhibit pruning.
- **Metric Pair:** N/A (Deterministic multi-plane intersection).

---

## 2. Dependencies & Prerequisites (`core`)

- **Direct Prerequisites:** `intake` (requires parsed target paths, diffs, or scope in `caseload.intake`, OR receives plenary `--all-canons` flag).
- **Transitive Prerequisites:** None.
- **Incoming Caseload:** Requires `caseload.intake` to be present (unless executing with `--all-canons`).

---

## 3. Core Functional Contract (`packages/core`)

```ts
export interface DiscoverOptions {
  readonly workspaceRoot: string;
  readonly canonGlobs?: readonly string[] | undefined;
  readonly explicitCanonFilter?: readonly string[] | undefined;
  readonly allCanons?: boolean | undefined;
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
  /** Mode of discovery: trigger-filtered or full corpus */
  readonly mode?: 'triggered' | 'all-canons' | undefined;

  /** Modified target file paths evaluated */
  readonly targetFiles: readonly string[];

  /** Discovered candidate canon paths matching targets (or full corpus) */
  readonly candidateCanons: readonly string[];

  /** Materialized active exhibits retained after mutual pruning with candidate canons */
  readonly activeExhibits: {
    readonly files: readonly string[];
    readonly prTitle?: boolean | undefined;
    readonly prBody?: boolean | undefined;
    readonly commitMessages?: boolean | undefined;
    readonly linkedIssues?: boolean | undefined;
  };

  /** Map of canon paths to matched target file paths and inspected planes */
  readonly triggersJoin: Record<string, readonly string[]>;
}
```

### Domain Short-Circuit Invariant
If `candidateCanons.length === 0`:
- The core engine sets `candidateCanons: []` and returns the enriched Caseload.
- Downstream execution ceases immediately, returning a clean success state without evaluating configuration or spending tokens.

---

## 4. Process & Domain Logic (`core`)

1. **Exhibit Materialization:** Resolves exhibit discovery directives from `caseload.intake` (e.g. expanding directory recursion roots and matching target glob patterns against workspace checkout into concrete file exhibits).
2. **Plenary Canon Discovery (`--all-canons`):** When `allCanons: true` is passed (via `--all-canons`), path trigger intersection is bypassed. Every discoverable canon in the workspace is placed directly into `candidateCanons` with `mode: 'all-canons'`. This establishes the full statutory corpus for downstream `validate --all-canons` ("Codex Audit").
   - **Two Hemispheres Invariant:** `--all-canons` operates exclusively on the *governing rule packs*, distinct from `--all-targets` (in `intake`), which operates on the *subject-matter codebase*.
3. **Canon Corpus Enumeration:** Discovers all candidate canons across the workspace (default pattern: `**/.canons/**/*.md`).
4. **Monorepo Subordination & Scope Containment:** Enforces strict boundary encapsulation:
   - Global canons in `.canons/**` apply repository-wide.
   - Scoped canons located in `<scope>/.canons/**` automatically inherit an implicit `<scope>/**` boundary:
     - Scoped canons are never evaluated against files outside `<scope>/`. If zero modified target files reside in `<scope>/`, the canon is excluded from candidates.
     - All declared paths and glob patterns in `triggers:`, `requires:`, and `references:` are evaluated strictly relative to `<scope>/`. Any pattern attempting directory traversal superior to `<scope>/` (e.g. `../`) is rejected as a validation error.
     - Scoped canons requiring files outside `<scope>/` must be hoisted to a parent `.canons/` directory.
5. **State Precondition Verification (`requires:`):** Evaluates declared `requires:` path patterns against the **Target File Tree** representing the final state being tested (PR `HEAD` commit in CI, filesystem working directory in local CLI, or speculative plan in dry-run modes):
   - All patterns in `requires:` MUST match at least one file present in the Target File Tree. If any pattern matches zero files, the canon is skipped / pruned with a diagnostic notice.
   - For scoped canons, `requires:` preconditions are evaluated strictly against the Target File Tree within `<scope>/`.
6. **Bipartite Exhibit Matching (Required vs. Optional Exhibits):**
   - **Required Planes (Default, e.g. `diff`, `pr_body`):** Evaluated strictly. A canon is pruned unless all of its declared required exhibit planes are satisfied in the intake filing (e.g. modified files matching `triggers:` or present metadata).
   - **Optional Planes (`?` Suffix, e.g. `pr_title?`, `pr_body?`):** Evaluated opportunistically. If present on `caseload.intake`, they are retained as active exhibits for the canon; if absent (such as in local CLI diff-only audits), their absence never causes the canon to be pruned.
   - **Default Frontmatter Ergonomics:** Canons with omitted `inspect` frontmatter default to `["diff", "pr_title?", "pr_body?"]`, guaranteeing that standard code rules apply seamlessly in local CLI runs (where `diff` is present but PR metadata is absent) without requiring dummy metadata flags.
7. **Two-Sided Mutual Pruning & Token Hygiene:**
   - **Inapplicable Canons Pruned:** Any canon whose `requires:` preconditions are unmet, or whose `triggers:` match 0 file exhibits, or whose required `inspect:` exhibit planes are unsatisfied is dropped $\implies$ `candidateCanons`.
   - **Un-inspected Exhibits Pruned:** Any tendered exhibit (file path, `pr_title`, `pr_body`, etc.) that is neither triggered nor inspected by any surviving candidate canon is dropped $\implies$ `activeExhibits`. Downstream screening (`docket`) and adjudication (`audit`) will never serialize un-inspected exhibits into model prompts.
8. **Join Calculation:** Computes `triggersJoin` mapping each candidate canon to the specific active exhibits that activated it.

---

## 5. Driving Adapter: CLI (`packages/cli`)

The CLI exposes `discover` as an imperative subcommand:

```bash
# Evaluate discovery on in-flight diff stream:
git diff origin/main | canon-clerk discover --diff -

# Enumerate all canons across the repository corpus:
canon-clerk discover --all-canons

# Evaluate discovery against an upstream Caseload:
canon-clerk discover --caseload caseload-1.json --json

# Predicate mode (-q):
git diff origin/main | canon-clerk discover -q -
```

### CLI Flags & Options
- `--all-canons`: Discovers and compiles all repository canons into candidateCanons, bypassing path trigger matching.
- `-q, --quiet`: Predicate mode. Suppresses stdout and indicates match presence via exit code.
- `--canon <path|glob>`: Explicitly restrict candidate canons to a specific subset.
- `--format <stylish|json|compact>`: Formats matched canons and triggering files.
- `--caseload <path|->`: Ingests upstream Caseload JSON.

### CLI Exit Codes
- **0:** Candidate canons matched and discovery attached (or `--all-canons` enumerated).
- **0 (Short-Circuit):** Zero candidate canons matched from diff stream; logs summary and exits immediately.
- **1 (Predicate Mode `-q`):** Exits 1 if zero candidate canons matched.
- **2:** Usage error, missing input source (naked invocation), or invalid glob syntax.

---

## 6. Driving Adapter: GitHub Action (`packages/action`)

1. **Automated Candidate Check:** Calls `executeDiscover` with the Caseload produced by `executeIntake`.
2. **Fast-Pass Evaluation:** If `candidateCanons.length === 0`, the action records a successful, neutral Check Run conclusion (`neutral` or `success`), logs that no governed files were touched, and terminates cleanly in <2 seconds without requiring `GEMINI_API_KEY`.
3. **Step Summary:** Emits a Markdown table of matched canons and triggering files into `GITHUB_STEP_SUMMARY`.

---

## 7. Driving Adapter: Integration Tests (`packages/integration-tests-private`)

Integration tests invoke `executeDiscover` directly against simulated monorepo directory layouts, asserting that scope inheritance (`packages/cli/.canons/` $\implies$ `packages/cli/**`) and complex glob patterns match accurately across OS platforms.
