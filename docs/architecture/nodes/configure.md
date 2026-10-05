# Stage 4: Configuration & Environment (`configure`)

**Status:** Authoritative Architectural Standard  
**Subcommand:** `canon-clerk configure`  
**Aliases:** `check-config`, `config`  
**Pipeline Track:** Branch B (The Environment Track)

---

## 1. Domain Concept & Role

`configure` serves as the **Operational Environment Normalizer** for Canon Clerk. In the court clerkship taxonomy, it represents the administrative court officer ensuring the courtroom facilities, judicial roster, and bailiff credentials are fully established before the court goes into session.

- **Imperative Verb:** `configure`
- **Court Clerkship Role:** Runtime environment resolution and provider credential validation.
- **Metric Pair:** N/A (Deterministic offline configuration resolution).

---

## 2. Dependencies

- **Direct Prerequisites:** None (Root node of Branch B).
- **Transitive Prerequisites:** None.
- **Pruned from Execution:** Branch A (`intake`, `discover`, `validate`), `probe`, Stages 5–7.
- **Independence:** Operates with **zero knowledge** of git diffs, changed files, or repository canons.

---

## 3. Specific Inputs

### Environment Variables & CLI Options
- `GEMINI_API_KEY`: Google Gemini provider API key.
- `--screener-model <model>`: Model identifier for macro screening (default: `google:gemini-3.5-flash-lite`).
- `--auditor-model <model>`: Model identifier for substantive adjudication (default: `google:gemini-3.8-pro`).
- `--reasoning-budget <tokens>`: Maximum reasoning budget tokens allocated to the adjudication stage.
- `--cwd <path>`: Explicit workspace root directory.
- `--caseload <path|->`: Incoming Caseload JSON (retained and enriched with `.config`).
- `--json`: Emits normalized configuration record or enriched Caseload.

---

## 4. Process & Logic

1. **Workspace Boundary Detection:** Locates project root, monorepo packages, and `.canons/` directories.
2. **Provider Credential Resolution:** Validates that `GEMINI_API_KEY` (or configured provider tokens) is present in the environment or configuration store.
3. **Model Specifier Normalization:** Resolves model tier aliases into fully-qualified model identifiers with provider namespaces.
4. **Offline Determinism:** Resolution is completely local and offline (<5ms), expending 0 tokens and making zero network requests.
5. **JIT Scheduling in Cascade:** When invoked as part of a review cascade (`docket`, `admit`, `audit`), `configure` is evaluated lazily *after* `discover` confirms matching candidate canons, guaranteeing that un-governed PRs pass cleanly without checking for credentials.

---

## 5. Outputs & Caseload Delta

Populates the `.config` field on the cumulative `Caseload`:

```ts
export interface CaseloadConfig {
  /** Model specifier for screening stages (e.g. 'google:gemini-3.5-flash-lite') */
  readonly screenerModel: string;

  /** Model specifier for adjudication (e.g. 'google:gemini-3.8-pro') */
  readonly auditorModel: string;

  /** Workspace root directory */
  readonly cwd: string;

  /** Optional reasoning budget in tokens */
  readonly reasoningBudget?: number | undefined;
}
```

### Caseload Delta
- `caseload.config`: Attached with normalized model specifiers, workspace path, and runtime parameters.

---

## 6. Gate & Error Semantics

- **Exit Code 0:** Configuration successfully validated and normalized.
- **Exit Code 2 (Configuration Error):**  
  **If required provider credentials (`GEMINI_API_KEY`) are missing, or model specifiers are invalid, execution fails fast with exit code `2`.**  
  *Prevents scheduling AI stages when the underlying environment is unconfigured.*
