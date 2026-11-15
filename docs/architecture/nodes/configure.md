# Configuration & Environment (`configure`)

**Status:** Authoritative Architectural Standard  
**Core Domain Engine:** Caseload Domain Engine  
**Driving Adapter:** CLI (`configure`)

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
    subgraph Inputs["Inputs"]
        Env["Host Environment<br/><i>(Env vars, secrets)</i>"]
        Params["Stage Flags<br/><i>(--screener-model, --auditor-model, --thinking-level, --reasoning-budget)</i>"]
    end

    Env --> Config["configure<br/><b>(Current Node)</b><br/><code>.config</code>"]
    Params --> Config

    Config --> Docket["docket<br/><i>(Adjudication Path)</i><br/><code>.docket</code>"]
    Config --> Apprise["apprise<br/><i>(Apprisal Path)</i><br/><code>.apprisal</code>"]
    Config -. "diagnostic probe" .-> Probe["probe<br/><i>(Diagnostic Path)</i><br/><code>.probe</code>"]
    Config -. "standalone CLI" .-> ConfigReport["Resolved Configuration<br/><i>(Terminus Happy Path, Exit 0)</i>"]
    Config -. "missing key / bad flags" .-> Error["Configuration Error<br/><i>(Exit 2, Abort)</i>"]

    style Config fill:#1f6feb,stroke:#58a6ff,stroke-width:2px,color:#fff
    style ConfigReport stroke-dasharray: 5 5
    style Probe stroke-dasharray: 5 5
    style Error stroke-dasharray: 5 5
```

- **Direct Prerequisites:** None (Root node of Branch B).
- **Transitive Prerequisites:** None.
- **Independence:** Resolves environment credentials and models with zero dependencies on diffs or canons.
- **Lazy Evaluation in Cascades:** When executed within a review cascade (`docket`, `admit`, `vet`, `audit`), `configure` is evaluated lazily *after* `discover` confirms matching candidate canons, guaranteeing that un-governed PRs pass cleanly without checking credentials.

---

## 3. Caseload Contribution & Data Plane Constraints

### Caseload Delta
`configure` populates the `.config` namespace on the cumulative `Caseload` envelope with normalized runtime provider settings:

- **Screener Model Specifier (`screener_model`):** Canonical provider-prefixed model identifier used by fast heuristic screening and prima facie vetting stages (`apprise`, `docket`, `admit`, `vet`; defaults to `"google:gemini-4-hydrogen"`).
- **Auditor Model Specifier (`auditor_model`):** Canonical provider-prefixed generative model identifier used for substantive trial adjudication (`audit`; defaults to `"google:gemini-4-carbon"`). Discriminative-only classifiers (`typesafe:*` / `jev`) are disallowed on `auditor_model` because `audit` requires structured prose decrees, `Remediation` synthesis, and 1-based `CodeAnnotation` spans.
- **Thinking Level (`thinking_level`):** Optional discrete qualitative reasoning effort tier (`"minimal"`, `"low"`, `"medium"`, `"high"`) governing extended thinking during `audit` trials (mutually exclusive with `reasoning_budget`; when both are omitted on `.config`, `audit` defaults to bounded `"medium"` thinking and `probe` defaults to `"low"`).
- **Reasoning Budget (`reasoning_budget`):** Optional integer token ceiling governing extended thinking during `audit` trials (mutually exclusive with `thinking_level`).

### Data Plane & Security Invariants
1. **Credential Non-Persistence & Zero Context Egress:** While `configure` verifies that required active-provider credentials (`GEMINI_API_KEY` / `providers.google.apiKey` and/or `TYPESAFE_API_KEY` / `providers.typesafe.apiKey`) are present, raw API keys and secret tokens are **never** serialized into `.config` on the `Caseload` envelope or echoed to terminal output, ensuring `caseload.json` artifacts and AI coding assistant tool traces remain safe from credential exfiltration.
2. **Workspace-External Credential Sourcing:** File-based configuration and credentials are sourced exclusively from user-level configuration directories outside the repository workspace envelope (`$XDG_CONFIG_HOME/canon-clerk/config.json` or `~/.config/canon-clerk/config.json` on Unix/macOS; `%APPDATA%\canon-clerk\config.json` on Windows). Any configuration file located inside the repository workspace is rejected with exit code `2` so workspace-scoped AI coding agents cannot read local secret files.
3. **POSIX Owner-Only Permission Enforcement:** On POSIX platforms, `configure` inspects file mode permissions before reading the user configuration file and fails fast with exit code `2` (suggesting `chmod 600`) if group or world permissions are granted (`(mode & 0o077) != 0`; only `0600` or `0400` are accepted).
4. **Canonical Multi-Provider Model Normalization:** Shorthand model aliases (`carbon`, `argon`, `hydrogen`, `flash-lite`, `flash`, `pro` for Google Gemini; `jev` for TypeSafe Jev), unprefixed model identifiers (`gemini-*` $\implies$ `google:gemini-*`, `jev-*` $\implies$ `typesafe:jev-*`), and explicit `<provider>:<model>` specifiers (`google:*`, `typesafe:*`) are normalized into canonical provider-prefixed specifiers prior to being recorded on `.config`.
5. **Mutual Exclusivity of Reasoning Controls:** `thinking_level` and `reasoning_budget` are mutually exclusive on `.config`. Specifying both at the same precedence tier (e.g., passing `--thinking-level` alongside `--reasoning-budget`, setting both `CANON_CLERK_THINKING_LEVEL` and `CANON_CLERK_REASONING_BUDGET`, or declaring both `thinkingLevel` and `reasoningBudget` in `config.json`) fails fast with exit code `2` and actionable guidance; across distinct precedence tiers, the higher-precedence tier supersedes and clears the lower-precedence alternative.

---

## 4. Process & Domain Logic (`configuration`)

1. **Hierarchical Precedence Cascade:** Resolves model specifiers, thinking levels / reasoning budgets, and provider credentials along a strict descending specificity cascade:
   - **Explicit CLI Flags:** `--screener-model`, `--auditor-model`, `--thinking-level`, `--reasoning-budget`, `--gemini-api-key`, `--typesafe-api-key`
   - **Environment Variables:** `CANON_CLERK_SCREENER_MODEL`, `CANON_CLERK_AUDITOR_MODEL`, `CANON_CLERK_THINKING_LEVEL`, `CANON_CLERK_REASONING_BUDGET`, `GEMINI_API_KEY`, `TYPESAFE_API_KEY`
   - **User-Level Configuration File (`~/.config/canon-clerk/config.json`):** Organized into orthogonal `providers` authentication blocks (`providers.google.apiKey`, `providers.typesafe.apiKey`) and optional tier and concurrency defaults (`screenerModel`, `auditorModel`, `thinkingLevel`, `reasoningBudget`, `appriseBatchSize`, `appriseConcurrency`, `docketBatchSize`, `docketConcurrency`, `admitConcurrency`, `vetConcurrency`, `auditConcurrency`)
   - **Static Defaults:** `google:gemini-4-hydrogen` (screener) and `google:gemini-4-carbon` (auditor)
2. **Model Specifier Normalization, Generative Auditor Enforcement, & Closure-Scoped Credential Gating:** Resolves shorthand aliases (`carbon` $\implies$ `google:gemini-4-carbon`, `argon` $\implies$ `google:gemini-4-argon`, `hydrogen` $\implies$ `google:gemini-4-hydrogen`, `flash-lite` $\implies$ `google:gemini-flash-lite-latest`, `flash` $\implies$ `google:gemini-flash-latest`, `pro` $\implies$ `google:gemini-pro-latest`, `jev` / `typesafe:jev` $\implies$ `typesafe:jev-latest`) and rejects unsupported provider prefixes, malformed model identifiers, conflicting thinking controls, or discriminative-only models (`typesafe:*`) on `auditor_model` *before* inspecting credentials. `configure` then verifies credentials strictly for the provider namespaces actively required by the invoked closure—requiring only `screener_model`'s provider key on screener-only closures (`apprise`, `docket`, `admit`, `vet` unless `--auditor-model` is explicitly overridden), and verifying both `screener_model` and `auditor_model` keys on `configure`, `probe`, and `audit`. Thus an environment provisioned solely with `TYPESAFE_API_KEY` can execute up through `canon-clerk vet --screener-model jev`, while `canon-clerk audit` requires a generative auditor credential (`GEMINI_API_KEY`).
3. **Deterministic & Offline:** Executes locally in <5ms without sending network requests or spending tokens.

---

## 5. Driving Adapter: CLI

The CLI exposes `configure` as an imperative subcommand:

```bash
# Normalize and print active configuration:
canon-clerk configure --json

# Override model tiers from flags (e.g., TypeSafe Jev screener + Gemini 4 Carbon auditor):
canon-clerk configure --screener-model jev --auditor-model carbon
```

### CLI Flags & Environment
- `--screener-model <model>` (fallback env: `CANON_CLERK_SCREENER_MODEL`): Custom screener model specifier or alias (`hydrogen`, `argon`, `carbon`, `flash-lite`, `flash`, `pro`, `jev`, `google:<model>`, `typesafe:<model>`).
- `--auditor-model <model>` (fallback env: `CANON_CLERK_AUDITOR_MODEL`): Custom generative auditor model specifier or alias (`carbon`, `argon`, `hydrogen`, `pro`, `flash`, `flash-lite`, `google:<model>`).
- `--thinking-level <minimal|low|medium|high>` (fallback env: `CANON_CLERK_THINKING_LEVEL`; config file key: `thinkingLevel`): Qualitative reasoning effort tier for `audit` trials (mutually exclusive with `--reasoning-budget`).
- `--reasoning-budget <tokens>` (fallback env: `CANON_CLERK_REASONING_BUDGET`; config file key: `reasoningBudget`): Maximum reasoning budget tokens (mutually exclusive with `--thinking-level`).
- `--gemini-api-key <key>` (fallback env: `GEMINI_API_KEY`): Google Gemini API key override.
- `--typesafe-api-key <key>` (fallback env: `TYPESAFE_API_KEY`): TypeSafe AI Jev API key override.
- `--format <stylish|json|compact>`: Output presentation format.
- `--caseload <path|->`: Ingests upstream Caseload.
- `--output-caseload <path>`: Writes cumulative `Caseload` JSON to disk independently of stdout.
- `--json`: Emits enriched Caseload JSON.

### CLI Exit Codes
- **0:** Configuration valid and normalized.
- **2:** Missing required active-provider credentials (`GEMINI_API_KEY` / `TYPESAFE_API_KEY`), insecure or workspace-resident configuration file, or invalid model/budget identifiers.
