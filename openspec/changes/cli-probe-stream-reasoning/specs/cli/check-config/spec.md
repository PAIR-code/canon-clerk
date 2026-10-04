# Spec Delta: cli/check-config

## ADDED Requirements

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
When active model probing exercises an endpoint that yields reasoning thoughts and token deltas, the probe result SHALL capture streaming latency metrics including Time-to-First-Thought (`timeToFirstThoughtMs`), Time-to-First-Token (`timeToFirstTokenMs`), and thought token count (`thoughtTokens`) in stylish and JSON outputs.

#### Scenario: Emitting streaming latency telemetry in stylish format
- **WHEN** probing a tier that generates thoughts and token deltas in stylish format
- **THEN** stylish output renders a dimmed Latency line showing time to first thought, reasoning duration, and token count

#### Scenario: Emitting streaming latency telemetry in JSON format
- **WHEN** invoking `canon-clerk check-config --probe --json` against a tier generating thoughts
- **THEN** the probe JSON object includes `timeToFirstThoughtMs`, `timeToFirstTokenMs`, and `thoughtTokens`
