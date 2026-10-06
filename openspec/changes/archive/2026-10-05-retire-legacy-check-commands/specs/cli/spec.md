# Spec Delta: Command-Line Interface

## MODIFIED Requirements

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
