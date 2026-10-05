# Stage 1: Change & Context Assembly (`intake`)

**Status:** Authoritative Architectural Standard  
**Subcommand:** `canon-clerk intake`  
**Pipeline Track:** Branch A (The Filing Track)

---

## 1. Domain Concept & Role

`intake` serves as the **Universal Front Door** of the Canon Clerk evaluation pipeline. In the court clerkship taxonomy, it represents the formal clerk receiving in-flight filings at the intake counter. It converts raw inputs—file paths, directory trees, patch streams, and pull request metadata—into standardized, immutable `FileArtifact` structures.

- **Imperative Verb:** `intake`
- **Court Clerkship Role:** Filing intake and document receipt.
- **Metric Pair:** N/A (Deterministic filing stage).

---

## 2. Dependencies

- **Direct Prerequisites:** None (Root node of Branch A).
- **Transitive Prerequisites:** None.
- **Execution Boundary:** **Zero child-process VCS execution.** Canon Clerk does not run `git` subprocesses internally; the operator or CI script feeds diffs and target paths directly via streams or flags.

---

## 3. Specific Inputs

### Standard Streams & Positional Arguments
- **Positional Path Arguments:** Direct file paths or globs:
  ```bash
  canon-clerk intake packages/cli/src/app.ts
  canon-clerk intake 'src/**/*.ts'
  ```
- **Positional `-` Stream:** Strictly reads newline-delimited file path tokens from `stdin` (xargs-style):
  ```bash
  git diff origin/main --name-only | canon-clerk intake -
  ```
- **`--diff <path|->` Stream:** Ingests a raw unified diff patch stream:
  ```bash
  git diff origin/main | canon-clerk intake --diff -
  canon-clerk intake --diff pr-42.patch
  ```
- **`--caseload <path|->`:** Ingests an existing serialized `Caseload` JSON to fast-forward upstream stages.

### Metadata Options
- `--pr-title <string>`: Pull request title or commit subject line.
- `--pr-body <string>`: Pull request markdown description.
- `--pr-body-file <path>`: Path to a file containing pull request markdown description.
- `--gh-pr <path|->`: Ingests GitHub API pull request JSON payload (e.g. from `gh pr view --json ...`).

---

## 4. Process & Logic

1. **Input Normalization:** Resolves positional target arguments, stdin path lists, or unified diffs into a unified list of touched files.
2. **Unified Diff Parsing:** If `--diff` is provided, parses patch hunks into `FileArtifact` objects detailing:
   - `path`: Normalized repository-relative path.
   - `status`: `'added' | 'modified' | 'deleted' | 'renamed' | 'copied' | 'unchanged'`.
   - `linesAdded` and `linesDeleted`: Line counts modified.
   - `patch`: Exact diff hunk content.
   - `contentOmissionReason`: Defaults to `'not_requested'` to preserve token hygiene.
3. **Filesystem Context Verification:** When positional paths are supplied without a patch, verifies file existence and stat metadata.
4. **Metadata Merging:** Binds `pr_title` and `pr_body` into the intake payload.

---

## 5. Outputs & Caseload Delta

Populates the `.intake` field on the cumulative `Caseload`:

```ts
export interface CaseloadIntake {
  /** PR title or commit subject */
  readonly pr_title?: string | undefined;

  /** PR markdown description or commit body */
  readonly pr_body?: string | undefined;

  /** Ingested code modifications keyed by relative repository path */
  readonly diffs: Record<string, FileArtifact>;
}
```

### Caseload Delta
- `caseload.intake`: Populated with parsed PR metadata and `FileArtifact` records for all modified files.

---

## 6. Gate & Error Semantics

- **Exit Code 0:** Successful intake; parsed files and diffs attached to Caseload.
- **Exit Code 2 (Usage / Malformed Input):**
  - Malformed unified diff stream that cannot be parsed.
  - Target file paths that do not exist or are inaccessible.
  - Conflicting stdin streams (e.g. attempting to read paths from `-` and diffs from `--diff -` simultaneously).
