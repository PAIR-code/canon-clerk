# Spec Delta

## MODIFIED Requirements
### Requirement: Subcommand Dispatch and Entrypoint Routing
The CLI binary SHALL route execution based on the first positional command argument. When executed with a recognized subcommand (`check-canons`, `check-triggers`), execution SHALL dispatch to the registered command handler. When executed without arguments, the CLI SHALL display the root help screen and exit with status 0. Invoking the retired `lint` subcommand SHALL exit with status 2 and remediation guidance to use `check-canons`.

#### Scenario: Dispatching to registered subcommand
- **WHEN** invoking `canon-clerk check-canons` with valid arguments
- **THEN** execution dispatches to the check-canons command handler

#### Scenario: Displaying root help on bare invocation
- **WHEN** invoking `canon-clerk` with no arguments
- **THEN** the root help screen is printed to stdout and the process exits with status 0

#### Scenario: Rejecting retired lint subcommand
- **WHEN** invoking `canon-clerk lint`
- **THEN** an error message is printed to stderr indicating unknown command and the process exits with status 2
