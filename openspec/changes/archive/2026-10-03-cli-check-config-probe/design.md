# Design: Active Model Endpoint Probing and Failure Disambiguation

## Context

Canon Clerk relies on LLM models in Phase 2 (Docket) and Phase 3 (Audit). The `canon-clerk check-config` command was introduced to inspect configuration cascade resolution and local credential store file permissions. However, verifying that a key exists on disk does not prove that the remote model is reachable, the key has not been revoked or expired, or the model identifier remains supported.

When connectivity issues occur, developers face varied failure modes:
1. Missing local credentials (pre-flight)
2. Malformed or expired API key (401 / 400 `API_KEY_INVALID`)
3. Unauthorized access, permissions missing, or API disabled in cloud console (403 `PERMISSION_DENIED` / `SERVICE_DISABLED`)
4. Malformed client payload or provider API contract change (400 `BAD_REQUEST`)
5. Deprecated or mistyped model identifier (404 `NOT_FOUND`)
6. Quota or rate limits exceeded (429 `RESOURCE_EXHAUSTED`)
7. Network down, DNS failure, or proxy failure (`fetch failed`, `ENOTFOUND`, `ECONNREFUSED`)
8. Unresponsive connection hanging indefinitely

## Goals / Non-Goals

**Goals:**
- Provide `--probe` on `check-config` to verify live end-to-end model reachability using a minimal structured query (`{ ok: boolean }`).
- Support configurable probe timeouts via `--probe-timeout <ms>` and `CANON_CLERK_PROBE_TIMEOUT_MS` (defaulting to 15,000ms).
- Codify the **Diagnostic Exemption Invariant** reconciling Phase 1's 0-token offline rule with diagnostic probing.
- Align defaults with evergreen `-latest` aliases and capture the provider's underlying concrete `resolvedModel` in probe output.
- Disambiguate errors into a clear, standardized `ProbeFailureCategory` taxonomy.
- Provide actionable remediation hints (URLs, env var suggestions) for each failure category.
- Establish `check-config --probe` as the universal diagnostic signpost for downstream cascade failures.
- Bound network interactions with a strict deadline timeout.
- Present clean, human-readable diagnostics in stylish mode and typed JSON fields in `--json`.

**Non-Goals:**
- Full semantic auditing or token benchmarking during `check-config`.
- Automatic key rotation or credential renewal.
- Interactive prompts to input API keys.

## Decisions

### 1. The Diagnostic Exemption Invariant
- **Decision:** Canon Clerk architectural invariants state: *"Phase 1: Check MUST NOT use AI. Must be 100% deterministic, local, and offline. 0 tokens."* We explicitly scope this invariant to **normal evaluation cascade operation** (automated PR gating and linting). Diagnostic subroutines invoked via explicit opt-in flags (`--probe`) are exempt because their purpose is verifying external dependencies and health for subsequent cascade phases.
- **Rationale:** Under normal operating conditions, `check-config` executes in 0 tokens and makes zero network calls. Invoking `--probe` is a conscious developer or CI maintenance action to test downstream plumbing.

### 2. Configurable Probe Timeout Deadline
- **Decision:** Introduce `--probe-timeout <ms>` CLI flag and `CANON_CLERK_PROBE_TIMEOUT_MS` environment variable. Defaults to 15,000ms.
- **Precedence:** `--probe-timeout` flag > `CANON_CLERK_PROBE_TIMEOUT_MS` env > default (15000).
- **Validation:** Must be a positive integer; non-integers or values <= 0 throw an `InvalidArgumentError` (exit code 2).
- **Rationale:** Enables fast-failing in automated CI pipelines (e.g. 3s–5s), accommodates high-latency proxy or mobile connections (30s+), and allows fast unit test verification of timeout handling.

### 3. Application-Level Semantic Ping vs. HTTP HEAD/GET Metadata
- **Decision:** The probe executes an end-to-end structured generation request (`POST :generateContent`) requesting `{ "ok": true }` constrained by schema `z.object({ ok: z.boolean() })`.
- **Rationale:** Acts as an application-level semantic ping testing the exact runtime path used by Phase 2 (Docket) and Phase 3 (Audit): authentication, model serving status, generation authorization, quota availability, and structured JSON decoding, while consuming a negligible number of tokens (~16 prompt tokens, ~5 completion tokens; <$0.000005).
- **Alternatives Considered & Rejected:**
  - *HTTP `HEAD` requests:* LLM inference endpoints universally reject `HEAD` requests with `405 Method Not Allowed`.
  - *Model Metadata `GET` requests (e.g. `GET /v1beta/models/{model}`):* Rejected due to high risk of false positives (a credential may have read permissions to inspect model metadata while lacking permissions or quota to run `generateContent`), provider coupling (bypasses the Vercel AI SDK / `ModelClient` abstraction, requiring provider-specific URL templates), and failure to verify structured JSON decoding capabilities.
  - *Unconstrained text generation:* Rejected because unconstrained text does not verify that the model satisfies JSON schema output constraints, which is the foundational contract required by Canon Clerk's evaluation cascade.

### 4. Dynamic Alias Defaults & Concrete Model Resolution
- **Decision:** Use dynamic `-latest` aliases for defaults in `@canon-clerk/core` (`google:gemini-flash-lite-latest` for screener and `google:gemini-flash-latest` for auditor). During probe execution, extract the provider's underlying concrete model identifier from the response (`modelVersion` in Google Generative AI) and report it as `resolvedModel` in `ModelTierProbeResult`.
- **Rationale:** Dynamic aliases prevent 404 deprecation breakage when upstream providers roll versions forward. Exposing `resolvedModel` gives developers full transparency into the exact snapshot actively answering queries (e.g. `gemini-3.5-flash-lite-001`), eliminating ambiguity while preserving zero-configuration durability.

### 5. Structured Failure Category Taxonomy
- **Decision:** Introduce `ProbeFailureCategory` in `@canon-clerk/configuration` with the following members:
  - `missing_credentials`: Intercepted locally before initiating network call.
  - `authentication`: HTTP 401 or 400 indicating invalid or expired API keys.
  - `authorization`: HTTP 403 indicating disabled API, missing IAM permissions, or billing failure.
  - `bad_request`: HTTP 400 other than authentication (e.g. malformed options or breaking API contract change).
  - `model_not_found`: HTTP 404 indicating model ID does not exist or has been deprecated.
  - `rate_limited`: HTTP 429 indicating quota exhaustion or concurrency throttling.
  - `network_error`: Transport-level errors (`ENOTFOUND`, `ECONNREFUSED`, fetch failure).
  - `timeout`: `TimeoutError` or `AbortError` after deadline.
  - `unknown`: Any uncategorized error.
- **Rationale:** Standardizes classification across different provider adapters (e.g. Google, Anthropic, Ollama) and decouples UI presentation from vendor-specific error messages.

### 6. Pre-flight Network Short-Circuit for Missing Credentials
- **Decision:** If `tierDiag.hasKey` is false (and provider is not credential-free like Ollama), fail immediately with `missing_credentials` at 0ms latency without making a doomed network request.
- **Rationale:** Prevents unnecessary network overhead and confusing error messages.

### 7. Error Disambiguation Parser
- **Decision:** Implement a robust error classification helper that inspects:
  - `err.statusCode` (from Vercel AI SDK `APICallError`)
  - `err.name` (`TimeoutError`, `AbortError`)
  - `err.cause` (Node.js network error codes: `ENOTFOUND`, `ECONNREFUSED`, `ETIMEDOUT`)
  - Substring signatures in `err.message` (e.g. `API_KEY_INVALID`, `SERVICE_DISABLED`, `RESOURCE_EXHAUSTED`).
- **Rationale:** Ensures accurate categorization regardless of whether the error originated from the SDK wrapper, Node fetch, or the upstream REST API.

### 8. Downstream Signposting
- **Decision:** When downstream cascade phases (Phase 2 Docket, Phase 3 Audit) encounter model invocation failures (authentication, network, rate limits), or when default `check-config` validates local configuration without probing, the CLI outputs an actionable hint directing users to `canon-clerk check-config --probe`.
- **Rationale:** Prevents developers from getting stranded by unhelpful vendor traces and establishes a single canonical troubleshooting tool across the repository.

## Risks / Trade-offs

- **Risk:** Provider error response formats may evolve over time.
  - **Mitigation:** Rely primarily on standard HTTP status codes (`statusCode`), falling back to substring signatures and finally `unknown`.
- **Risk:** Probing incurs latency and minor API token consumption.
  - **Mitigation:** Probing is opt-in via `--probe`, uses the smallest possible prompt and schema (`{ ok: boolean }`), and bounds timeouts to a configurable deadline (default 15 seconds).
