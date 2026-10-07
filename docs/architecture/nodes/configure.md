# Configuration & Environment (`configure`)

**Status:** Authoritative Architectural Standard  
**Core Domain Engine:** Caseload Domain Engine  
**Driving Adapters:** CLI (`configure`), GitHub Action, Integration Tests

---

## 1. Domain Concept & Role (`core` / `configuration`)

`configure` serves as the **Operational Environment Normalizer** for Canon Clerk. In the court clerkship taxonomy, it represents the administrative court officer ensuring the courtroom facilities, judicial roster, and bailiff credentials are fully established before the court goes into session.

- **Imperative Verb:** `configure`
- **Court Clerkship Role:** Runtime environment resolution and provider credential validation.
- **Metric Pair:** N/A (Deterministic offline configuration resolution).

---

## 2. Dependencies & Prerequisites (`core`)

```mermaid
flowchart LR
    Env["Host Environment & Workspace<br/><i>(Env vars, CLI flags, credential stores)</i>"] --> Config["configure<br/><b>(Current Node)</b><br/><code>.config</code>"]
    Config --> Docket["docket / apprise<br/><i>(Converges with Branch A)</i>"]
    Config -. "diagnostic probe" .-> Probe["probe<br/><code>.probe</code>"]

    style Config fill:#1f6feb,stroke:#58a6ff,stroke-width:2px,color:#fff
```

- **Direct Prerequisites:** None (Root node of Branch B).
- **Transitive Prerequisites:** None.
- **Independence:** Resolves environment credentials and models with zero dependencies on diffs or canons.
- **Lazy Evaluation in Cascades:** When executed within a review cascade (`docket`, `admit`, `audit`), `configure` is evaluated lazily *after* `discover` confirms matching candidate canons, guaranteeing that un-governed PRs pass cleanly without checking credentials.

---

## 3. Core Functional Contract

```text
struct ConfigureOptions:
  cwd?: String
  env?: Map[String, String]
  overrides?: CaseloadConfigOverrides

function execute_configure(
  options: ConfigureOptions,
  caseload?: Caseload
) -> Caseload
```

### Caseload Delta
Populates the `.config` field on the cumulative `Caseload`:

```text
struct CaseloadConfig:
  // Model specifier for screening nodes (e.g. 'google:gemini-3.5-flash-lite')
  screener_model: String

  // Model specifier for adjudication (e.g. 'google:gemini-3.8-pro')
  auditor_model: String

  // Workspace root directory
  cwd: String

  // Optional reasoning budget in tokens
  reasoning_budget?: Integer
```

---

## 4. Process & Domain Logic (`configuration`)

1. **Workspace Boundary Detection:** Identifies workspace root, monorepo packages, and configuration files.
2. **Credential Resolution:** Checks environment variables (`GEMINI_API_KEY`) and secure platform secret stores.
3. **Model Specifier Normalization:** Resolves model aliases (e.g. `flash-lite` $\implies$ `google:gemini-3.5-flash-lite`, `pro` $\implies$ `google:gemini-3.8-pro`).
4. **Deterministic & Offline:** Executes locally in <5ms without sending network requests or spending tokens.

---

## 5. Driving Adapter: CLI

The CLI exposes `configure` as an imperative subcommand:

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

## 6. Driving Adapter: GitHub Action

1. **Secret & Input Mapping:** Maps workflow inputs (`api-key`, `screener-model`, `auditor-model`, `reasoning-budget`) and repository secrets into `ConfigureOptions`.
2. **JIT Invocation:** Evaluates `executeConfigure` only after `executeDiscover` yields $>0$ candidate canons.
3. **Missing Secret Reporting:** If `GEMINI_API_KEY` is absent on a PR requiring AI adjudication, posts an actionable failure annotation instructing maintainers to configure repository secrets for fork PRs.

---

## 7. Driving Adapter: Integration Tests

Integration tests invoke `executeConfigure` with test environment variables and scoped credentials, verifying that model configurations resolve properly before executing live network requests.
