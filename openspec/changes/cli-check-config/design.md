# Design

## Context

Canon Clerk evaluates engineering rules across a Three-Phase Evaluation Cascade: Phase 1 (deterministic checks), Phase 2 (screener), and Phase 3 (auditor). Configuration resolution cascades through a 6-tier descending hierarchy: programmatic overrides > tier environment variables > general environment variables > vendor environment variables > host credential store (`~/.config/canon-clerk/config.json`) > static canonical defaults.

While this hierarchy guarantees flexibility and zero-friction onboarding, it creates operational opacity. Developers and automated pipelines cannot easily determine which model will be invoked, where the resolution originated, or whether local credential stores conform to host security invariants.

## Goals / Non-Goals

**Goals:**
- Provide a dedicated, zero-token diagnostic subcommand (`canon-clerk check-config`) adhering to Phase 1 performance constraints.
- Report granular source attribution for each resolved property (model, reasoning effort, API key, base URL).
- Audit host credential store permissions against POSIX owner-only security standards without breaking read-only configurations.
- Mask all secrets at presentation boundaries to prevent credential leakage.
- Support dual output modalities (`stylish` human tree and `json` machine payload).
- Provide predictable exit codes for CI gating and shell scripting.

**Non-Goals:**
- Performing online network calls or API token verification against external LLM providers (belongs to an online connectivity check, not Phase 1 deterministic validation).
- Mutating configuration files or setting keys (belongs to configuration writer workflows).

## Decisions

### 1. Phase 1 Deterministic Diagnostic Boundary
- **Decision:** Implement `check-config` as a zero-token, purely deterministic offline diagnostic tool.
- **Rationale:** Aligns with Canon Clerk's Three-Phase cascade architecture where Phase 1 tools run in milliseconds at zero token cost. Offline verification avoids network timeouts, rate limits, and failure in air-gapped CI environments.
- **Alternatives Considered:** Performing a live "ping" or "test-prompt" to each provider. Rejected because network latency and credential billing violate Phase 1 deterministic performance guarantees.

### 2. POSIX Owner-Only Bitmask Permission Audit
- **Decision:** Audit POSIX permissions by verifying that group and world access bits are unset (`(mode & 0o077) === 0`), permitting owner-only modes (`0o600` or read-only `0o400`), while bypassing octal checks on non-POSIX platforms.
- **Rationale:** Enforcing exact `0o600` creates false positives in environments where credential files are mounted read-only (such as container secrets mounted with mode `0o400`). Bitmask testing ensures defense-in-depth against multi-user exfiltration without penalizing more restrictive permissions.
- **Alternatives Considered:** Strict `0o600` equality. Rejected due to false failures on valid read-only mounts.

### 3. Granular Property-Level Provenance Attribution
- **Decision:** Track and report the resolution source individually for each configuration property (`model`, `effort`, `apiKey`, `baseURL`) rather than assigning a single source to the entire tier.
- **Rationale:** In layered configuration systems, properties frequently resolve from different tiers (e.g. model from tier-specific env vars, API key from OS credential store, and base URL from static defaults). Collapsing provenance to a single tier-level source obscures operational root causes.
- **Alternatives Considered:** Tier-level source summary. Rejected because it cannot represent mixed-source configurations.

### 4. Warning Sensitivity and Exit Code Gating
- **Decision:** Preserve exit code 0 when only advisory warnings are present (such as ambient multi-key presence), and provide `--max-warnings <count>` to enable strict failure gating when requested.
- **Rationale:** Developers frequently have ambient API keys for multiple providers set in their shells. If ambient warnings trigger non-zero exits by default, routine developer checks and CI pre-flight runs fail needlessly.
- **Alternatives Considered:** Exiting with code 1 on any warning. Rejected because it conflates benign warnings with fatal misconfigurations.

### 5. Trailing-Only Secret Redaction
- **Decision:** Redact API keys displaying only trailing characters (e.g. `...4x9Z`) or an abstract presence badge (`[SET]`).
- **Rationale:** Fixed prefixes (such as Google's `AIza`) carry zero secret entropy, while displaying both leading and trailing characters on short keys reveals up to half of the secret space. Trailing-only masking provides confirmation of which key is active without leaking substantial entropy.
- **Alternatives Considered:** Revealing first-4 and last-4 characters. Rejected due to entropy leakage on short tokens.

## Risks / Trade-offs

- **Platform Divergence on Windows:** Windows does not implement POSIX permission bits, meaning file mode auditing cannot detect ACL over-permissiveness. Mitigation: Gracefully bypass octal auditing on non-POSIX platforms while continuing file existence and parse checks.
- **Offline Credential Validity:** An API key may be present and correctly formatted but revoked upstream. Mitigation: Clearly document that `check-config` is a Phase 1 structural and presence check; online verification is handled during Phase 2/3 cascade execution.
