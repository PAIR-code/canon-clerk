# Canon Check-Config CLI Specification

## Purpose

Defines the Phase 1 configuration health and diagnostic subcommand (`canon-clerk check-config`), cascade resolution inspection, secret masking invariants, host credential store permission verification, terminal formatting, canonical JSON reporting, and deterministic exit codes.

## Requirements

### Requirement: Cascade Resolution and Provenance Attribution
The `check-config` command SHALL resolve and report configuration for evaluation cascade tiers (`screener` and `auditor`), including provider, model name, reasoning effort, base URL, credential presence, and granular source attribution for each resolved property (`tier-env`, `general-env`, `vendor-env`, `store`, or `default`). When `--tier <name>` is passed, inspection SHALL be restricted to the specified tier.

#### Scenario: Inspecting default cascade configuration
- **WHEN** invoking `canon-clerk check-config` with default environment
- **THEN** screener and auditor tiers are evaluated and reported with their resolved providers, models, and sources

#### Scenario: Filtering inspection to a specific tier
- **WHEN** invoking `canon-clerk check-config --tier screener`
- **THEN** only the screener tier configuration is inspected and reported

### Requirement: Secret Masking Invariant
The `check-config` command SHALL ensure plaintext API keys and authentication tokens are never emitted to stdout, stderr, or JSON reports. Discovered credentials SHALL be truncated and masked displaying only trailing characters (`...xxxx`) or an abstract status indicator (`[SET]`).

#### Scenario: Masking discovered credentials in terminal output
- **WHEN** inspecting a configuration containing an active API key
- **THEN** the credential is displayed with only trailing characters visible and the remainder redacted

#### Scenario: Masking discovered credentials in JSON reports
- **WHEN** invoking `canon-clerk check-config --json` with an active API key
- **THEN** the output JSON contains masked key representations and never contains plaintext secret values

### Requirement: Host Credential Store and Permission Verification
The `check-config` command SHALL inspect the host credential store path, existence, and byte length. On POSIX operating systems, the command SHALL audit file mode permissions to verify owner-only access (`(mode & 0o077) === 0`), rejecting group or world read/write access. On operating systems lacking native POSIX permission semantics (such as Windows), octal mode verification SHALL be safely bypassed.

#### Scenario: Auditing secure owner-only credential file permissions
- **WHEN** inspecting a credential store with mode 0o600 or 0o400 on a POSIX platform
- **THEN** permission verification reports secure status and valid configuration

#### Scenario: Flagging overly permissive credential file permissions
- **WHEN** inspecting a credential store with group or world access permissions on a POSIX platform
- **THEN** permission verification reports an insecure status and emits an actionable security warning

#### Scenario: Safely bypassing permission checks on non-POSIX platforms
- **WHEN** inspecting a credential store on Windows
- **THEN** POSIX octal mode auditing is bypassed while host file presence is preserved

### Requirement: Output Formatting and Machine-Readable JSON
When configured with `--format stylish` (default), the `check-config` command SHALL emit a human-readable diagnostic tree displaying store status, permission health, and cascade resolution details. When configured with `--format json` or `--json`, the command SHALL emit canonical unadorned JSON containing host store diagnostics, per-tier configurations with granular property sources, warnings, and overall validity.

#### Scenario: Emitting human-readable diagnostic tree in stylish format
- **WHEN** invoking `canon-clerk check-config` with default format
- **THEN** formatted text is emitted displaying store status and tier resolution trees

#### Scenario: Emitting structured canonical JSON report
- **WHEN** invoking `canon-clerk check-config --json`
- **THEN** valid unadorned JSON is emitted containing store metadata, tier configurations with sources, and validity status

### Requirement: Deterministic Exit Codes and Warning Thresholds
The `check-config` command SHALL exit with status 0 when configuration is valid, active tier credentials exist, and permissions are secure. The command SHALL exit with status 1 on missing credentials, insecure permissions, fatal errors, or when warnings exceed `--max-warnings <count>`. When `--quiet` (`-q`) is passed, stdout SHALL be suppressed. Argument syntax errors SHALL exit with status 2.

#### Scenario: Passing valid configuration with exit status 0
- **WHEN** inspecting a completely configured environment with secure permissions
- **THEN** the process exits with status 0

#### Scenario: Failing on missing credentials with exit status 1
- **WHEN** inspecting an active tier lacking required provider credentials
- **THEN** the failure is reported and the process exits with status 1

#### Scenario: Failing when warnings exceed max-warnings threshold
- **WHEN** invoking `canon-clerk check-config --max-warnings 0` in an environment with advisory warnings
- **THEN** the process exits with status 1

#### Scenario: Operating silently in quiet mode
- **WHEN** invoking `canon-clerk check-config -q`
- **THEN** stdout is suppressed and process exit status indicates configuration validity

### Requirement: Command Line Usage and Help Screen
The `check-config` command SHALL support `--help` (`-h`). When requested, the command SHALL output categorized usage instructions, options, and runnable invocation examples.

#### Scenario: Displaying check-config help screen
- **WHEN** invoking `canon-clerk check-config --help`
- **THEN** categorized help with runnable examples is printed to stdout and the process exits with status 0
