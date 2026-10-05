# Stage 4: Configuration & Environment (`configure`)

**Status:** Authoritative Architectural Standard  
**Stage:** 4  
**Core Domain Engine:** `@canon-clerk/configuration` (with `@canon-clerk/core`)  
**Driving Adapters:** `@canon-clerk/cli` (`configure`, `check-config`), `@canon-clerk/action`, `@canon-clerk/integration-tests-private`

---

## 1. Domain Concept & Role (`core` / `configuration`)

`configure` serves as the **Operational Environment Normalizer** for Canon Clerk. In the court clerkship taxonomy, it represents the administrative court officer ensuring the courtroom facilities, judicial roster, and bailiff credentials are fully established before the court goes into session.

- **Imperative Verb:** `configure`
- **Court Clerkship Role:** Runtime environment resolution and provider credential validation.
- **Metric Pair:** N/A (Deterministic offline configuration resolution).

---

## 2. Dependencies & Prerequisites (`core`)

- **Direct Prerequisites:** None (Root node of Branch B).
- **Transitive Prerequisites:** None.
- **Independence:** Resolves environment credentials and models with zero dependencies on diffs or canons.
- **Lazy Evaluation in Cascades:** When executed within a review cascade (`docket`, `admit`, `audit`), `configure` is evaluated lazily *after* `discover` confirms matching candidate canons, guaranteeing that un-governed PRs pass cleanly without checking credentials.

---

## 3. Core Functional Contract (`packages/configuration`)

```ts
export interface ConfigureOptions {
  readonly cwd?: string | undefined;
  readonly env?: Record<string, string | undefined> | undefined;
  readonly overrides?: Partial<CaseloadConfig> | undefined;
}

export function executeConfigure(
  options: ConfigureOptions,
  caseload?: Caseload | undefined
): Promise<Caseload>;
```

### Caseload Delta
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

---

## 4. Process & Domain Logic (`configuration`)

1. **Workspace Boundary Detection:** Identifies workspace root, monorepo packages, and configuration files.
2. **Credential Resolution:** Checks environment variables (`GEMINI_API_KEY`) and secure platform secret stores.
3. **Model Specifier Normalization:** Resolves model aliases (e.g. `flash-lite` $\implies$ `google:gemini-3.5-flash-lite`, `pro` $\implies$ `google:gemini-3.8-pro`).
4. **Deterministic & Offline:** Executes locally in <5ms without sending network requests or spending tokens.

---

## 5. Driving Adapter: CLI (`packages/cli`)

The CLI exposes `configure` (aliased as `check-config` and `config`):

```bash
# Normalize and print active configuration:
canon-clerk configure --json

# Override model tiers from flags:
canon-clerk configure --screener-model google:gemini-3.5-flash-lite --auditor-model google:gemini-3.8-pro
```

### CLI Flags & Environment
- `GEMINI_API_KEY`: Google Gemini API key.
- `--screener-model <model>`: Custom screener model specifier.
- `--auditor-model <model>`: Custom auditor model specifier.
- `--reasoning-budget <tokens>`: Maximum reasoning budget tokens.
- `--cwd <path>`: Explicit workspace directory.
- `--caseload <path|->`: Ingests upstream Caseload.

### CLI Exit Codes
- **0:** Configuration valid and normalized.
- **2:** Missing provider credentials (`GEMINI_API_KEY`), invalid model identifiers, or unresolvable workspace path.

---

## 6. Driving Adapter: GitHub Action (`packages/action`)

1. **Secret & Input Mapping:** Maps workflow inputs (`api-key`, `screener-model`, `auditor-model`, `reasoning-budget`) and repository secrets into `ConfigureOptions`.
2. **JIT Invocation:** Evaluates `executeConfigure` only after `executeDiscover` yields $>0$ candidate canons.
3. **Missing Secret Reporting:** If `GEMINI_API_KEY` is absent on a PR requiring AI adjudication, posts an actionable failure annotation instructing maintainers to configure repository secrets for fork PRs.

---

## 7. Driving Adapter: Integration Tests (`packages/integration-tests-private`)

Integration tests invoke `executeConfigure` with test environment variables and scoped credentials, verifying that model configurations resolve properly before executing live network requests.
