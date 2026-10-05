# Stage 3: Canon Rule Linter (`validate`)

**Status:** Authoritative Architectural Standard  
**Subcommand:** `canon-clerk validate`  
**Aliases:** `check-canons`, `lint`  
**Pipeline Track:** Branch A (The Filing Track)

---

## 1. Domain Concept & Role

`validate` serves as the **Statutory Validity Gate** of the pipeline. In the court clerkship taxonomy, it represents the clerk reviewing submitted legal statutes and citations to verify that the law is correctly codified, has not suffered textual corruption, and conforms to jurisdictional formatting standards before being cited in court.

- **Imperative Verb:** `validate`
- **Court Clerkship Role:** Pre-flight linting and AST verification of canon rule files.
- **Metric Pair:** N/A (Deterministic static analysis).

---

## 2. Dependencies

- **Direct Prerequisites:**
  - Targeted mode: `discover` (validates matched candidate canons).
  - Standalone mode: None (validates all canons in `.canons/**`).
- **Transitive Prerequisites:** `intake` (targeted mode only).
- **Pruned from Execution:** Branch B (`configure`), `probe`, Stages 5–7.

---

## 3. Specific Inputs

### Standard Streams & CLI Options
- Incoming `Caseload` via `--caseload <path|->` (containing `caseload.discovery.candidateCanons`).
- Raw canon content via `stdin` (`canon-clerk validate -`).
- `--max-warnings <number>`: Threshold for allowable warnings before failing.
- `--format <stylish|json|compact>`: Output formatting choice.
- `--json`: Emits the updated `Caseload` containing the `.validation` block.

---

## 4. Process & Logic

1. **Target Selection:**
   - In targeted cascade execution, extracts `candidateCanons` from `caseload.discovery`.
   - In standalone mode (e.g. `npm run check-canons`), discovers all workspace canons (`**/.canons/**/*.md`).
2. **AST Parsing:** Parses Markdown body and optional YAML frontmatter delimiters (`---`).
3. **Static Rule Verification:**
   - **Frontmatter Schema:** Validates YAML structure and types (`id`, `title`, `triggers`, `inspect`, `tags`, `references`).
   - **RFC 2119 Formulations:** Verifies normative keyword usage (`MUST`, `SHOULD`, etc.) in uppercase.
   - **Canon Naming Standard:** Verifies invariant slug naming conventions (`canon-names-must-state-invariants`).
   - **Rule Atomicity:** Ensures single-rule cohesion; flags compound invariants.
   - **Directive Headers:** Validates `Exception`, `Rationale`, and `Remediation` directive blocks.
4. **Result Aggregation:** Assembles per-canon diagnostic results (errors, warnings) and computes overall `hasErrors`.

---

## 5. Outputs & Caseload Delta

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

### Caseload Delta
- `caseload.validation`: Attached with validation results per canon and `hasErrors` flag.

---

## 6. Gate & Error Semantics

- **Exit Code 0:** All evaluated canons pass static linting (or warnings are below `--max-warnings`).
- **Fail-Fast Exit Code 1 (Validation Error):**  
  **If any candidate canon contains a syntax or schema violation, execution terminates immediately with exit code `1`.**  
  *Zero AI tokens, zero network requests, and zero model credentials are spent if canons are malformed.*
- **Exit Code 2 (Usage / Configuration Error):** Invalid CLI options, unresolvable paths, or I/O failure reading files.
