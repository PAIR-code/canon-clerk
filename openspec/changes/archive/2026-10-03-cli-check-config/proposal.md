# Proposal

## Why

With the introduction of `@canon-clerk/configuration` in #176, model cascade settings and API credentials resolve along a 6-tier descending hierarchy of specificity (programmatic overrides, tier-specific environment variables, general environment variables, vendor environment variables, OS credential store, and static defaults).

Currently, developers, CI runners, and AI coding assistants cannot deterministically inspect configuration state on the command line:
- Identifying which provider, model, and base URL are active for Phase 2 (Screener) and Phase 3 (Auditor).
- Attributing the exact origin of resolved settings (CLI overrides, environment variables, credential store, or defaults).
- Verifying whether host API keys are present without leaking plaintext secrets into terminal output or CI logs.
- Verifying that host credential store files (`~/.config/canon-clerk/config.json`) enforce owner-only permissions per `.canons/security/credential-readers-must-reject-insecure-permissions.md`.
- Detecting multi-key provider ambiguities or missing credentials prior to executing LLM evaluations.

In accordance with Phase 1's deterministic `check-*` verb taxonomy (`check-canons`, `check-triggers`) and `.canons/architecture/format-validators-must-be-pure-evaluators.md`, Canon Clerk requires a dedicated zero-token diagnostic subcommand: `canon-clerk check-config`.

## What Changes

- Introduce the `check-config` subcommand under `@canon-clerk/cli` to inspect cascade configuration, secret presence, and credential store permissions at zero token cost.
- Implement cascade configuration inspection and granular source attribution for both `screener` and `auditor` tiers (tracking origin for model, reasoning effort, API key, and base URL).
- Enforce secret masking across all diagnostic reporters, truncating credentials to trailing 4 characters (`...xxxx`) or boolean status (`[SET]`).
- Implement host credential store permission verification on POSIX systems using an owner-only bitmask audit (`(mode & 0o077) === 0`, accepting `0o600` and `0o400`), while safely handling non-POSIX operating systems (Windows).
- Implement human-readable `stylish` tree formatting and canonical `json` reporting (`--format json`, `--json`).
- Provide filtering by tier (`--tier <name>`), silent predicate execution (`-q, --quiet`), and customizable warning sensitivity (`--max-warnings <count>`).
- Establish deterministic exit codes: 0 for healthy configuration, 1 for missing credentials or permission violations, and 2 for invalid arguments.

## Capabilities

### New Capabilities
- `cli/check-config`: Configuration diagnostics subcommand, cascade resolution source attribution, secret masking, host credential store permission audit, and deterministic exit codes.

## Impact

- `@canon-clerk/cli`: Registers the new `check-config` command in the root CLI router and adds associated diagnostic formatters.
- `@canon-clerk/configuration`: Supplies or enhances credential store inspection and resolution provenance reporting.
- Tooling and CI workflows: Provides zero-token pre-flight validation in local development and CI review gates.
