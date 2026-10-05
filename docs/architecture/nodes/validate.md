# Statutory Rule Linter (`validate`)

**Status:** Authoritative Architectural Standard  
**Core Domain Engine:** `@canon-clerk/core` (with `@canon-clerk/schema`)  
**Driving Adapters:** `@canon-clerk/cli` (`validate`, `check-canons`, `lint`), `@canon-clerk/action`, `@canon-clerk/integration-tests-private`

---

## 1. Domain Concept & Role (`core`)

`validate` serves as the **Statutory Validity Gate** of the pipeline. In the court clerkship taxonomy, it represents the clerk reviewing submitted legal statutes and citations to verify that the law is correctly codified, has not suffered textual corruption, and conforms to jurisdictional formatting standards before being cited in court.

- **Imperative Verb:** `validate`
- **Court Clerkship Role:** Pre-flight linting and AST verification of canon rule files.
- **Metric Pair:** N/A (Deterministic static analysis).

---

## 2. Dependencies & Prerequisites (`core`)

- **Direct Prerequisites:**
  - Targeted mode: `discover` (validates matched candidate canons from `caseload.discovery`).
  - Standalone mode: None (validates all canons in `.canons/**`).
- **Transitive Prerequisites:** `intake` (targeted mode only).

---

## 3. Core Functional Contract (`packages/core`)

```ts
export interface ValidateOptions {
  readonly workspaceRoot: string;
  readonly canonsToValidate?: readonly string[] | undefined;
  readonly maxWarnings?: number | undefined;
}

export function executeValidate(
  options: ValidateOptions,
  caseload?: Caseload | undefined
): Promise<Caseload>;
```

### Caseload Delta
Populates the `.validation` field on the cumulative `Caseload`:

```ts
export interface ValidatedCanonMetadata {
  readonly result: 'pass' | 'fail';
  readonly warningCount: number;
  readonly warnings: readonly string[];
  readonly errorCount: number;
  readonly errors: readonly string[];
}

export interface CaseloadValidation {
  /** Map of canon paths to AST/schema validation metadata */
  readonly results: Record<string, ValidatedCanonMetadata>;

  /** True if any candidate canon contains lint errors */
  readonly hasErrors: boolean;
}
```

### Domain Error Invariant
If `hasErrors === true`, the validation result records the failure diagnostics and signals an immediate abort before loading environment credentials or spending tokens.

---

## 4. Process & Domain Logic (`core`)

1. **Target Selection:**
   - In targeted cascade mode, extracts `candidateCanons` from `caseload.discovery`.
   - In standalone mode, parses and validates all discoverable canons in the workspace.
2. **AST Parsing via `@canon-clerk/schema`:** Parses Markdown AST and YAML frontmatter blocks.
3. **Static Rule Verification:**
   - **Frontmatter Schema:** Validates YAML types (`id`, `title`, `triggers`, `inspect`, `tags`, `references`).
   - **RFC 2119 Formulations:** Verifies normative keyword usage (`MUST`, `SHOULD`, etc.) in uppercase.
   - **Canon Naming Standard:** Enforces invariant slug naming conventions (`canon-names-must-state-invariants`).
   - **Rule Atomicity:** Ensures single-rule cohesion; flags compound invariants.
   - **Directive Headers:** Validates `Exception`, `Rationale`, and `Remediation` directive blocks.
4. **Diagnostic Assembly:** Assembles structured warnings and errors per canon.

---

## 5. Driving Adapter: CLI (`packages/cli`)

The CLI exposes `validate` (aliased as `check-canons` and `lint`):

```bash
# Standalone workspace lint:
canon-clerk validate
canon-clerk check-canons

# Targeted validation within a pipeline:
canon-clerk validate --caseload caseload-2.json --json

# Lint raw canon Markdown from stdin:
cat .canons/rule.md | canon-clerk check-canons -
```

### CLI Flags & Environment
- `--max-warnings <n>`: Warning threshold before triggering non-zero exit code.
- `--format <stylish|json|compact>`: Output formatting choice.
- `--quiet`: Suppress warnings and non-essential output.
- `--caseload <path|->`: Ingests upstream Caseload.

### CLI Exit Codes
- **0:** All evaluated canons pass static linting.
- **1:** Validation errors detected, or warnings exceed `--max-warnings`.
- **2:** Usage error or file access failure.

---

## 6. Driving Adapter: GitHub Action (`packages/action`)

1. **Pre-Flight Validation:** Executes `executeValidate` on all candidate canons identified during `discover`.
2. **Annotation Generation:** Converts syntax or schema errors into GitHub Actions error annotations (`::error file=path,line=n::message`), pointing PR authors to the exact line of the malformed canon.
3. **Fail-Fast:** If `hasErrors === true`, posts a failing Check Run conclusion and stops the action run before contacting model providers.

---

## 7. Driving Adapter: Integration Tests (`packages/integration-tests-private`)

Integration tests invoke `executeValidate` on fixture canons containing deliberately malformed frontmatter, non-atomic invariants, and invalid RFC 2119 syntax, verifying that AST errors are captured deterministically across test runners.
