# Command-Line Interface Specification

## Purpose

Defines the base command-line interface entrypoint, subcommand routing, global options, signal handling, I/O separation, and standardized process exit codes for `canon-clerk`.

## Requirements

### Requirement: Subcommand Dispatch and Entrypoint Routing
The CLI binary SHALL route execution based on registered command handlers. When executed without command arguments, the CLI SHALL display the root help screen and exit with status 0. When executed with an unrecognized command or the retired `lint` command, the CLI SHALL print an error message with remediation guidance to stderr and exit with status 2.

#### Scenario: Dispatching to registered subcommand
- **WHEN** invoking `canon-clerk` with a registered command
- **THEN** execution dispatches to the registered command handler

#### Scenario: Displaying root help on bare invocation
- **WHEN** invoking `canon-clerk` with no arguments
- **THEN** the root help screen is printed to stdout and the process exits with status 0

#### Scenario: Rejecting retired lint subcommand
- **WHEN** invoking `canon-clerk lint`
- **THEN** an error message is printed to stderr indicating unknown command and the process exits with status 2

#### Scenario: Rejecting unrecognized subcommand
- **WHEN** invoking `canon-clerk unknown-command`
- **THEN** an error message is printed to stderr indicating unknown command and the process exits with status 2

### Requirement: Global Options and Information Flags
The CLI SHALL support global options `--help` (`-h`) and `--version` (`-v`). When `--version` is requested, the CLI SHALL print the CLI package version to stdout and exit with status 0. When `--help` is requested, the CLI SHALL display categorized usage documentation and exit with status 0.

#### Scenario: Querying CLI version
- **WHEN** invoking `canon-clerk --version` or `canon-clerk -v`
- **THEN** the CLI version string is printed to stdout and the process exits with status 0

#### Scenario: Querying root help
- **WHEN** invoking `canon-clerk --help` or `canon-clerk -h`
- **THEN** command usage and available subcommands are printed to stdout and the process exits with status 0

### Requirement: Standard Process Signal Handling
The CLI process SHALL register handlers for termination signals (`SIGINT` and `SIGTERM`). When a termination signal is received during command execution, the process SHALL terminate with standard POSIX signal exit status (`130` for `SIGINT`, `143` for `SIGTERM`) without unhandled stack traces.

#### Scenario: Terminating gracefully on SIGINT
- **WHEN** the CLI process receives a `SIGINT` signal
- **THEN** the process exits immediately with status 130 without printing unhandled errors

#### Scenario: Terminating gracefully on SIGTERM
- **WHEN** the CLI process receives a `SIGTERM` signal
- **THEN** the process exits immediately with status 143 without printing unhandled errors

### Requirement: Standardized Process Exit Codes
The CLI SHALL terminate with standardized exit codes: status 0 for successful execution or acceptable warning counts, status 1 for domain audit or lint violations, status 2 for invalid CLI arguments or unknown subcommands, and status 3 or greater for uncaught runtime exceptions.

#### Scenario: Exiting on invalid CLI argument syntax
- **WHEN** invoking the CLI with an unrecognized option or malformed argument
- **THEN** an error message is printed to stderr and the process exits with status 2

#### Scenario: Exiting on unknown subcommand
- **WHEN** invoking the CLI with an unrecognized subcommand
- **THEN** an error message with remediation guidance is printed to stderr and the process exits with status 2

### Requirement: Separation of Output Data from Diagnostics
The CLI SHALL emit primary payload data and formatted command reports exclusively to stdout. Operational diagnostics, execution errors, usage errors, and progress indicators MUST be directed strictly to stderr.

#### Scenario: Emitting primary payload to stdout
- **WHEN** a command generates structured data or formatted inspection results
- **THEN** the output is written directly to stdout

#### Scenario: Directing usage errors to stderr
- **WHEN** command invocation fails due to invalid syntax or an unknown flag
- **THEN** the error description and remediation hint are written directly to stderr
