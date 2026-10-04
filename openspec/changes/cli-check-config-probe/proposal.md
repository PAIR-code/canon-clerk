# Proposal: Active Model Endpoint Probing with Failure Disambiguation

## Why

Phase 1 (`check-config`) validates configuration syntax, credential existence, and file permissions, but cannot confirm whether configured model endpoints are genuinely reachable or whether API credentials have valid authorization. Developers and CI workflows encounter obscure errors later during Phase 2 (Docket) or Phase 3 (Audit) if credentials are expired, unauthorized, rate-limited, or misconfigured.

Furthermore, remote failures stem from distinct root causes:
1. Missing local credentials (pre-flight)
2. Malformed or expired API keys (Authentication)
3. Permissions or disabled APIs (Authorization)
4. Outdated client contracts or invalid payloads (Bad Request)
5. Deprecated or misspelled models (Model Not Found)
6. Quota or rate exhaustion (Rate Limited)
7. Transport and DNS failures (Network Error)
8. Unresponsive endpoints (Timeout)

Without active endpoint probing and structured failure disambiguation, developers must decipher raw provider error payloads and stack traces. Additionally, when using evergreen dynamic aliases (like `-latest`), developers need visibility into which concrete model snapshot was resolved by the provider.

## What Changes

1. **The Diagnostic Exemption Invariant:**
   - Codify an explicit architectural distinction for Phase 1 `check-*` commands: under normal operating conditions, commands are strictly local, deterministic, and 0 tokens. Opt-in diagnostic flags (`--probe`) are explicitly exempt because their purpose is verifying external plumbing required by subsequent cascade phases.

2. **Active Probing Flag (`--probe`) and Configurable Timeout (`--probe-timeout`):**
   - Add `--probe` flag to `canon-clerk check-config`.
   - Issue minimal schema-constrained test queries (`{ ok: boolean }`) against active tiers (`screener`, `auditor`, or filtered via `--tier`).
   - Add `--probe-timeout <ms>` flag and `CANON_CLERK_PROBE_TIMEOUT_MS` environment variable, defaulting to 15,000ms.
   - Bound probe execution with an explicit timeout signal (`AbortSignal.timeout(timeoutMs)`).

3. **Dynamic Alias Defaults & Resolved Model Attribution:**
   - Align default models in `@canon-clerk/core` to evergreen dynamic aliases (`google:gemini-flash-lite-latest` for screener and `google:gemini-flash-latest` for auditor), preventing 404 deprecation rot.
   - Extract the provider's resolved concrete model snapshot (`modelVersion` from the provider response) and report it in probe results (`resolvedModel`).

4. **Failure Disambiguation Taxonomy:**
   - Define structured `ProbeFailureCategory` across `@canon-clerk/configuration` and `@canon-clerk/cli`:
     - `missing_credentials`
     - `authentication`
     - `authorization`
     - `bad_request`
     - `model_not_found`
     - `rate_limited`
     - `network_error`
     - `timeout`
     - `unknown`
   - Classify errors based on HTTP status codes, error causes, and provider message signatures.
   - Attach actionable remediation hints to failure diagnostics (e.g. key generation URL, permission hints).

5. **Signposting & Triage Guidance:**
   - Establish `canon-clerk check-config --probe` as the canonical diagnostic destination across Canon Clerk.
   - Suggest `--probe` in `check-config` output hints when configuration is valid but unprobed.
   - Establish signposting conventions for downstream cascade commands (Phase 2 & 3) to direct users to `--probe` when encountering model connectivity errors.

6. **Terminal and Machine-Readable Reporting:**
   - Stylish tree output: Colorized success checkmarks with latency and resolved concrete model snapshot, or categorized failure details with root cause and actionable remediation hints.
   - JSON report (`--json`): Embed typed probe object containing `ok`, `durationMs`, `resolvedModel`, `category`, `error`, and `hint`.

7. **Deterministic Exit Codes:**
   - Exit 0 if all probed tiers succeed.
   - Exit 1 if any probed tier fails reachability or authorization.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `cli/check-config`: Add `--probe` and `--probe-timeout <ms>` options, active endpoint reachability verification, failure classification taxonomy, resolved model snapshot attribution, actionable remediation hints, diagnostic exemption rationale, and probe-sensitive exit codes.

## Impact

- **CLI (`@canon-clerk/cli`):** Enhances `check-config` command options, probe runner, and formatters.
- **Core (`@canon-clerk/core`):** Aligns default models to dynamic `-latest` aliases and captures `resolvedModel` in structured generation.
- **Configuration (`@canon-clerk/configuration`):** Extends `ModelTierDiagnostics` and `ModelTierProbeResult` type definitions with `resolvedModel` and `ProbeFailureCategory`.
