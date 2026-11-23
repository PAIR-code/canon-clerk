# Provider Profiles, Capabilities & Decision Budgets

**Status:** Authoritative Architectural Standard  
**Domain Concept:** Decoupled Provider Profiles, Capability Ports, `DecisionBudgetProfile` Matrices & Shared Provider Controls

---

## 1. Architectural Role & Orthogonal Separation

Canon Clerk decouples **who hosts an endpoint** (a provider profile `<slug>`) from **what wire protocol it speaks** (`gemini` or `systemone`) and **what domain capability port a model satisfies** (`Discriminator` or `Generator`).

This document is the single authoritative specification for built-in and custom provider profiles, canonical `<provider>:<model>` grammar, discriminative context/batching budgets (`DecisionBudgetProfile`), per-profile execution defaults, and shared provider/credential/endpoint/reasoning CLI and environment cascades. Related cross-cutting contracts reside in:
- **[`nodes/configure.md`](nodes/configure.md):** Branch B runtime resolution, workspace-external `config.json` schema & POSIX `0600` security, 4-level node model cascade, and closure-exact validation.
- **[`transport.md`](transport.md):** Shared pooled HTTPS reactor, wire-protocol HTTP routing, per-origin cooldown gating, bounded retry taxonomy, and timeout/retry controls (`--timeout`, `--max-retries`, `--max-backoff-ms`).
- **[`scheduling.md`](scheduling.md):** Stage-prefixed CLI flag closure scoping and lazy `configure` scheduling.

---

## 2. Canonical `<provider>:<model>` Grammar & Protocol-Bound Capabilities

Every model specifier across CLI flags, environment variables, `config.json` defaults/node pins, and `.config.nodes` records MUST conform to canonical `<provider>:<model>` syntax with non-empty provider and model segments separated by a single `:` delimiter and no whitespace.

- **Rejected Legacy / Ambiguous Forms (Exit Code `2`):** Bare short-name prefix inference (`gemini-decision-1`, `gemini-*`, `jev-*`), retired pet-name aliases (`google:carbon`, `google:hydrogen`, `typesafe:jev`), unversioned `google:gemini-decision`, and structured `{ provider, model }` object shims fail fast with an actionable remediation `Hint:`.
- **Protocol-Bound Capability Classification:** Model syntax validation, `<provider>:` prefix stripping, and `Discriminator` vs. `Generator` capability classification bind to the profile's wire protocol (`gemini` or `systemone`) rather than hardcoded vendor slugs:
  - **`gemini` Protocol Profiles (`google` and custom `"protocol": "gemini"` profiles):** Classify `<slug>:gemini-decision-*` as `Discriminator` and other `<slug>:gemini-*` identifiers as `Generator`.
  - **`systemone` Protocol Profiles (`typesafe`, `local`, and custom `"protocol": "systemone"` profiles):** Classify `<slug>:<model>` strictly as `Discriminator` (`systemone` profiles do not provide `Generator` models and reject `defaultGenerator` with exit code `2`). Built-in `typesafe` validates `jev-*` model identifiers, whereas built-in `local` and custom `systemone` profiles accept any non-empty `[A-Za-z0-9._/-]+` `<model>` identifier (such as `local:kev-4b`, `local:kev-0.8b`, or `lan-gpu:clef-flash`, while rejecting generative `gemini-*` names).
- **Stage Capability Requirements:**
  - **Screening & Vetting Nodes (`apprise`, `docket`, `admit`, `vet`):** Strictly require a `Discriminator` model. Configuring any `Generator` model on these nodes fails fast with exit code `2` and an actionable remediation hint pointing to supported discriminator providers (`google:gemini-decision-*`, `typesafe:jev-*`, or `local:*`).
  - **Judicial Adjudication Node (`audit`):** Strictly requires a `Generator` model (to synthesize `Exception` rulings, `Remediation` prose, and 1-based `CodeAnnotation` spans). Configuring any `Discriminator` model on `audit` fails fast with exit code `2`.

---

## 3. Built-In & Custom Provider Profiles

Ambient `*_API_KEY` environment variables never implicitly switch the active provider or models. The active default provider resolves deterministically along a strict specificity cascade:

$$\texttt{--provider <slug>} \;\succ\; \texttt{CANON\_CLERK\_PROVIDER} \;\succ\; \texttt{config.json defaults.provider} \;\succ\; \texttt{"google"}$$

Unrecognized provider slugs (not matching a built-in profile or a configured `providers.<slug>` profile in `config.json`) fail fast with exit code `2` and an actionable remediation hint.

| Profile Slug | Wire Protocol | Default `baseUrl` | Auth Policy | Default `Discriminator` (`apprise`..`vet`) | Default `Generator` (`audit`) |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `google` *(built-in default)* | `gemini` | `https://generativelanguage.googleapis.com/v1beta` | Required API key (`x-goog-api-key`) | `google:gemini-decision-1` | `google:gemini-4-carbon` |
| `typesafe` *(built-in)* | `systemone` | `https://api.typesafe.ai/v1` | Required API key (`Bearer`) | `typesafe:jev-latest` | *(none)* |
| `local` *(built-in)* | `systemone` | `http://127.0.0.1:8080/v1` | **Optional API key** (`Bearer` when set; unauthenticated otherwise) | `local:kev-4b` | *(none)* |
| Custom `<slug>` (`"protocol": "systemone"`) | `systemone` | Explicit `providers.<slug>.baseUrl` | **Optional API key** (`Bearer` when set; unauthenticated otherwise) | `providers.<slug>.defaultDiscriminator` *(optional)* | *(none)* |
| Custom `<slug>` (`"protocol": "gemini"`) | `gemini` | Explicit `providers.<slug>.baseUrl` | Required API key (`x-goog-api-key`) | `providers.<slug>.defaultDiscriminator` *(optional)* | `providers.<slug>.defaultGenerator` *(optional)* |

### Profile Declaration Invariants
- **Built-In Profiles (`google`, `typesafe`, `local`):** Inherit their built-in wire protocol, default `baseUrl`, authentication policy (`RequiredApiKey` on `google`/`typesafe`; `OptionalApiKey` on `local`), and default per-capability models when omitted in `config.json`, and reject conflicting `protocol` overrides with exit code `2`.
- **Zero-Code Custom Profiles (e.g., `local-dev`, `lan-gpu`, `google-corp`):** Must use lowercase kebab-case `<slug>` identifiers (`[a-z0-9]+(-[a-z0-9]+)*`) and explicitly declare both `protocol` (`"gemini"` or `"systemone"`) and `baseUrl` (`http://` or `https://` URL with a non-empty host authority), failing fast with exit code `2` and an actionable `Hint:` when omitted or malformed. When `defaultDiscriminator` or `defaultGenerator` is declared on a profile, it must carry the matching `<slug>:` prefix and satisfy the corresponding capability.
- **Unprovisioned `audit` on Discriminator-Only Profiles:** When the active profile defines no `defaultGenerator` (`typesafe`, `local`, or a custom `systemone` profile) and `audit` is unpinned across Levels 1–3 ([`nodes/configure.md` §4](nodes/configure.md#4-process--domain-logic-configuration)), `audit` remains unprovisioned—allowing `apprise`..`vet` and standalone `configure`/`probe` to succeed cleanly while failing fast with exit code `2` if generative `audit` execution is required.

---

## 4. `DecisionBudgetProfile` & Per-Profile Execution Defaults

Discriminative context/question-batching budgets (`DecisionBudgetProfile`) and stage execution defaults bind to the resolved provider profile and wire protocol:

| Profile / Protocol Tier | Upstream Ceiling | Max State Chars | Max Batch Chars | Max `nouls` / Call | Default Screening `batchSize` (`apprise`, `docket`, `admit`) | Default `concurrency` (`apprise`..`audit`) |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `gemini` `Discriminator` (`google:gemini-decision-*` & custom `gemini`) | `262,144` tokens / `1,000` questions | `240,000` | `480,000` | `256` | `256` | `4` |
| Cloud `systemone` `Discriminator` (`typesafe:jev-*` & custom `systemone`) | `32,768` tokens | `36,000` | `64,000` | `48` | `48` | `4` |
| Built-in `local` `systemone` (`local:*`) | Local GGUF context | `24,000` | `32,000` | `16` | `2` | `1` |
| `gemini` `Generator` (`audit`, e.g., `google:gemini-4-carbon`) | Generative context | — | — | — | N/A *(rejected)* | `4` |

- **`vet` Bifurcated Span Budget:** `vet` packs paired (`compliant`, `claims_exception`) `nouls` within `Max nouls / Call` and `Max Batch Chars` (`256` / `480,000` on `gemini`; `48` / `64,000` on cloud `systemone`; `16` / `32,000` on `local:*` so single-batch span localization stays active on typical local diffs) before falling back to unfenced file-level vetting ([`nodes/vet.md` §4](nodes/vet.md#4-process--domain-logic-core)).
- **Transport Timeout & Retry Defaults:** Per-attempt wire timeouts (`60,000ms` cloud / `180,000ms` `local:*`) and retry/cooldown bounds (`--timeout`, `--max-retries`, `--max-backoff-ms`) are specified authoritatively in [`transport.md` §6](transport.md#6-out-of-band-telemetry--cli-transport-controls).

### Operational Knob Specificity Cascades

| Knob | Specificity Cascade | Built-In / Profile Default | Envelope Persistence |
| :--- | :--- | :--- | :--- |
| `threshold` | `--<node>-threshold` $\succ$ `CANON_CLERK_<NODE>_THRESHOLD` $\succ$ `nodes.<node>.threshold` $\succ$ default | `0.80` on `vet`; `0.50` on `apprise`, `docket`, `admit`, `audit` | Execution parameter (not in `.config`) |
| `batchSize` *(`apprise`, `docket` only)* | `--<node>-batch-size` $\succ$ `CANON_CLERK_<NODE>_BATCH_SIZE` $\succ$ `nodes.<node>.batchSize` $\succ$ profile default | `256` (`gemini`), `48` (cloud `systemone`), `2` (`local:*`) | Execution parameter (not in `.config`; rejected on `admit`, `vet`, `audit`) |
| `concurrency` | `--<node>-concurrency` $\succ$ `CANON_CLERK_<NODE>_CONCURRENCY` $\succ$ `nodes.<node>.concurrency` $\succ$ profile default | `4` (cloud profiles); `1` (`local:*`) | Execution parameter (not in `.config`) |
| `thinkingLevel` / `reasoningBudget` | `--thinking-level` / `--reasoning-budget` $\succ$ `CANON_CLERK_THINKING_LEVEL` / `CANON_CLERK_REASONING_BUDGET` $\succ$ `nodes.<node>.*` $\succ$ `defaults.*` $\succ$ stage default | `"low"` (`includeThoughts: true`) on `audit` | Recorded on `.config.nodes.audit` when explicitly set |

---

## 5. Shared Provider, Credential, Endpoint & Thinking Controls

### Credential & Endpoint Specificity Cascades
For any referenced profile `<slug>` (with `<UPPER_SLUG>` formed by uppercasing `<slug>` and replacing `-` with `_`), transport credentials and endpoint URLs resolve independently along deterministic specificity cascades:

- **API Key Precedence:** Built-in CLI flag (`--gemini-api-key` on `google`, `--typesafe-api-key` on `typesafe`, `--local-api-key` on `local`) $\succ$ `CANON_CLERK_<UPPER_SLUG>_API_KEY` $\succ$ `<UPPER_SLUG>_API_KEY` (plus `GEMINI_API_KEY` on `google`) $\succ$ `providers.<slug>.apiKey` in `config.json`. Profiles with a required API-key policy fail fast with exit code `2` when no key resolves; `local` and custom `systemone` profiles omit the `Authorization` header when no key resolves so keyless localhost `/v1/systemone` daemons work without dummy tokens.
- **Base URL Precedence:** Built-in CLI flag (`--gemini-base-url` on `google`, `--typesafe-base-url` on `typesafe`, `--local-base-url` on `local`) $\succ$ `CANON_CLERK_<UPPER_SLUG>_BASE_URL` $\succ$ `<UPPER_SLUG>_BASE_URL` (plus `GEMINI_BASE_URL` on `google`) $\succ$ `providers.<slug>.baseUrl` in `config.json` $\succ$ built-in profile default. Resolved URLs must begin with `http://` or `https://` and name a non-empty host authority, failing fast with exit code `2` during `configure` when malformed.

### Reasoning Control Mutual Exclusivity & Capability Scoping
`thinking_level` and `reasoning_budget` are mutually exclusive within any single configuration layer (CLI flags, environment variables, `nodes.<node>`, or `defaults`), failing fast with exit code `2` when both appear at the same layer; across distinct precedence layers, the higher-precedence layer supersedes and clears the lower-precedence alternative. Shared/ambient thinking controls (`--thinking-level`, `--reasoning-budget`, `CANON_CLERK_THINKING_LEVEL`, `CANON_CLERK_REASONING_BUDGET`, `defaults.thinkingLevel`, `defaults.reasoningBudget`) apply strictly to `Generator`-bound nodes and leave `Discriminator`-bound nodes unset, whereas explicitly declaring per-node `nodes.<node>.thinkingLevel` or `nodes.<node>.reasoningBudget` on a node whose resolved model is a `Discriminator` fails fast with exit code `2`.

### Shared CLI Flags & Environment Variables
Accepted across `configure`, `probe`, and all five heuristic evaluation subcommands (`apprise`, `docket`, `admit`, `vet`, `audit`, where evaluation subcommands resolve them JIT only when heuristic execution is required):

| CLI Flag | Fallback Environment Variable(s) | `config.json` Key | Semantics |
| :--- | :--- | :--- | :--- |
| `--provider <SLUG>` | `CANON_CLERK_PROVIDER` | `defaults.provider` | Active default provider profile slug (`google`, `typesafe`, `local`, or custom `providers.<slug>`; default: `google`). |
| `--<node>-model <PROVIDER:MODEL>` | `CANON_CLERK_<NODE>_MODEL` | `nodes.<node>.model` | Per-node canonical `<provider>:<model>` override (`apprise`, `docket`, `admit`, `vet`, `audit`; closure-scoped on evaluation subcommands per [`scheduling.md` §4](scheduling.md#pattern-c-the-stage-prefixed-flag-scoping-contract); all five accepted on `configure` and `probe`; retired `--screener-model` / `--auditor-model` rejected with exit code `2`). |
| `--thinking-level <minimal\|low\|medium\|high>` | `CANON_CLERK_THINKING_LEVEL` | `nodes.<node>.thinkingLevel`, `defaults.thinkingLevel` | Qualitative reasoning effort tier for `Generator`-bound nodes (mutually exclusive with `--reasoning-budget`). |
| `--reasoning-budget <TOKENS>` | `CANON_CLERK_REASONING_BUDGET` | `nodes.<node>.reasoningBudget`, `defaults.reasoningBudget` | Maximum reasoning budget tokens for `Generator`-bound nodes (mutually exclusive with `--thinking-level`). |
| `--gemini-api-key <KEY>` | `CANON_CLERK_GOOGLE_API_KEY`, `GEMINI_API_KEY` | `providers.google.apiKey` | Google Gemini API key override. |
| `--typesafe-api-key <KEY>` | `CANON_CLERK_TYPESAFE_API_KEY`, `TYPESAFE_API_KEY` | `providers.typesafe.apiKey` | TypeSafe AI Jev API key override. |
| `--local-api-key <KEY>` | `CANON_CLERK_LOCAL_API_KEY`, `LOCAL_API_KEY` | `providers.local.apiKey` | Optional Bearer token override for the built-in `local` System One profile. |
| `--gemini-base-url <URL>` | `CANON_CLERK_GOOGLE_BASE_URL`, `GEMINI_BASE_URL` | `providers.google.baseUrl` | Google Generative Language API base URL override. |
| `--typesafe-base-url <URL>` | `CANON_CLERK_TYPESAFE_BASE_URL`, `TYPESAFE_BASE_URL` | `providers.typesafe.baseUrl` | TypeSafe AI System One API base URL override. |
| `--local-base-url <URL>` | `CANON_CLERK_LOCAL_BASE_URL`, `LOCAL_BASE_URL` | `providers.local.baseUrl` | Local System One daemon base URL override (default: `http://127.0.0.1:8080/v1`). |

*(Custom `providers.<slug>` profiles additionally support `CANON_CLERK_<UPPER_SLUG>_API_KEY` / `<UPPER_SLUG>_API_KEY` and `CANON_CLERK_<UPPER_SLUG>_BASE_URL` / `<UPPER_SLUG>_BASE_URL` environment overrides.)*
