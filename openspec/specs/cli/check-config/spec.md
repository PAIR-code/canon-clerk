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

### Requirement: Active Model Endpoint Probing
When `--probe` is passed, the `check-config` command SHALL actively exercise the production model generation API for configured tiers to verify live inference authorization, quota availability, and service reachability. The probe SHALL NOT rely on shallow metadata queries, unauthenticated pings, or simulated health endpoints. The command SHALL enforce a bounded timeout, defaulting to 15,000ms, overridable via `--probe-timeout <ms>` or `CANON_CLERK_PROBE_TIMEOUT_MS`.

#### Scenario: Successfully probing reachable model endpoint
- **WHEN** invoking `canon-clerk check-config --probe` with valid credentials and network reachability
- **THEN** actively exercises the production generation API, reports reachability status with roundtrip latency in milliseconds, and exits 0

#### Scenario: Attributing resolved concrete model snapshot
- **WHEN** probing a model tier configured with an alias where the provider returns underlying model version metadata
- **THEN** captures the underlying model identifier in `resolvedModel` and reports it in stylish and JSON outputs

#### Scenario: Failing probe on missing credentials
- **WHEN** invoking `canon-clerk check-config --probe` without configured provider credentials
- **THEN** fails without network call, reports `missing_credentials` category, and exits with status 1

#### Scenario: Overriding probe timeout deadline via CLI flag
- **WHEN** invoking `canon-clerk check-config --probe --probe-timeout 5000`
- **THEN** enforces a 5,000ms deadline on model endpoint probing

#### Scenario: Overriding probe timeout via environment variable
- **WHEN** invoking `canon-clerk check-config --probe` with `CANON_CLERK_PROBE_TIMEOUT_MS=8000`
- **THEN** enforces an 8,000ms deadline on model endpoint probing

#### Scenario: Rejecting invalid probe timeout values
- **WHEN** invoking `canon-clerk check-config --probe --probe-timeout abc` or negative value
- **THEN** throws an invalid argument error and exits with code 2

### Requirement: Probe Failure Disambiguation
The `check-config --probe` command SHALL classify probe errors into structured failure categories (`missing_credentials`, `authentication`, `authorization`, `bad_request`, `model_not_found`, `rate_limited`, `network_error`, `timeout`, or `unknown`) and provide actionable remediation hints.

#### Scenario: Classifying authentication failure
- **WHEN** probing an endpoint with an invalid or expired API key resulting in an authentication error
- **THEN** reports failure with category `authentication` and a remediation hint directing to credential setup

#### Scenario: Classifying authorization failure
- **WHEN** probing an endpoint with a credential lacking required permissions or disabled API
- **THEN** reports failure with category `authorization` and a permission remediation hint

#### Scenario: Classifying model not found error
- **WHEN** probing a tier configured with an unrecognized or deprecated model identifier
- **THEN** reports failure with category `model_not_found` and a model configuration hint

#### Scenario: Classifying network and transport errors
- **WHEN** probing an endpoint when DNS fails or connection cannot be established
- **THEN** reports failure with category `network_error` and a network connectivity hint

#### Scenario: Enforcing probe deadline timeout
- **WHEN** probing an endpoint that hangs beyond the configured timeout deadline
- **THEN** aborts execution, reports failure with category `timeout`, and exits with status 1

### Requirement: Streaming Reasoning Thoughts During Endpoint Probing
When `--probe` is executed, the probe runner SHALL invoke the streaming model client endpoint (`streamStructured`) when supported, echoing reasoning thoughts in real time to stderr while awaiting the completion verdict. When `--no-thoughts` is passed, `CANON_CLERK_PROBE_THOUGHTS=false` is set, or quiet mode (`-q`) is enabled, thought streaming SHALL be suppressed.

#### Scenario: Streaming reasoning thoughts during active probe
- **WHEN** probing an active model tier that yields thought events
- **THEN** thought deltas are written to stderr in real time while awaiting the verdict payload

#### Scenario: Silencing thoughts via CLI flag
- **WHEN** invoking `canon-clerk check-config --probe --no-thoughts`
- **THEN** thought streaming is suppressed on stderr while the final diagnostic report is printed to stdout

#### Scenario: Silencing thoughts via environment variable
- **WHEN** invoking `canon-clerk check-config --probe` with `CANON_CLERK_PROBE_THOUGHTS=false`
- **THEN** thought streaming is suppressed on stderr while the final diagnostic report is printed to stdout

#### Scenario: Preserving clean JSON output on stdout during streaming
- **WHEN** invoking `canon-clerk check-config --probe --json` against a tier emitting thoughts
- **THEN** thoughts are directed to stderr and stdout emits strictly valid JSON

#### Scenario: Suppressing thoughts in quiet mode
- **WHEN** invoking `canon-clerk check-config --probe -q`
- **THEN** no thought output is written to stderr or stdout

### Requirement: Streaming Latency Telemetry Attribution
When active model probing exercises an endpoint that yields reasoning thoughts and token deltas, the probe result SHALL capture streaming latency metrics including Time-to-First-Thought (`timeToFirstThoughtMs`), Time-to-First-Token (`timeToFirstTokenMs`), thought token count (`thoughtTokens`), and thought chunk count (`thoughtChunks`) in stylish and JSON outputs. When an endpoint emits zero thought chunks, the output SHALL explicitly disambiguate by reporting `(0 thoughts)` in stylish format and `thoughtChunks: 0` in JSON format.

#### Scenario: Emitting streaming latency telemetry in stylish format
- **WHEN** probing a tier that generates thoughts and token deltas in stylish format
- **THEN** stylish output renders a dimmed Latency line showing time to first thought, reasoning duration, token count, and chunk count

#### Scenario: Disambiguating zero thought packets in stylish format
- **WHEN** probing a tier that emits zero thought events during streaming
- **THEN** stylish output renders a dimmed Latency line with `(0 thoughts)` suffix

#### Scenario: Emitting streaming latency telemetry in JSON format
- **WHEN** invoking `canon-clerk check-config --probe --json` against a tier generating thoughts
- **THEN** the probe JSON object includes `timeToFirstThoughtMs`, `timeToFirstTokenMs`, `thoughtTokens`, and `thoughtChunks`


