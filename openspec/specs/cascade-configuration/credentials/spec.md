# cascade-configuration/credentials Specification

## Purpose

Defines host operating system credential management outside the repository workspace envelope, providing secure user-level secret storage, owner-only file permissions, and non-throwing missing store diagnostics.

## Requirements

### Requirement: Workspace-Piercing OS Credential Storage
The system SHALL store and retrieve host-level credentials in standard operating system user configuration directories, isolated from the repository workspace envelope.

#### Scenario: Resolving standard OS configuration path
- **WHEN** resolving the credential store path on the host platform
- **THEN** returns an absolute path locating `config.json` within the standard OS user configuration directory

#### Scenario: Reading stored credentials
- **WHEN** querying a stored credential key from an existing store
- **THEN** returns the trimmed credential string, or undefined if the key is unset or empty

#### Scenario: Writing and deleting stored credentials
- **WHEN** updating or deleting a credential key in the store
- **THEN** persists the updated key-value mapping or removes the key from the underlying storage file

#### Scenario: Reading structured provider configurations
- **WHEN** querying a provider configuration block from `providers.<name>`
- **THEN** returns the typed provider configuration containing `apiKey` and optional `baseURL`

### Requirement: Owner-Only Credential Access Permissions
The system SHALL enforce POSIX `0o600` permissions on local credential files, ensuring credential stores are readable and writable solely by the executing user.

#### Scenario: Creating credential store with restricted permissions
- **WHEN** a new credential store file is written to the host filesystem
- **THEN** the file is created with owner-only read and write permissions (`0o600`)

### Requirement: Non-Throwing Missing Store Diagnostics
The system SHALL safely inspect credential store status and fail safe without throwing unhandled exceptions when credential files or directories do not exist.

#### Scenario: Safe inspection of missing credential store
- **WHEN** inspecting diagnostics for a credential store file that does not exist
- **THEN** returns a diagnostic record with `exists: false` without throwing an exception

#### Scenario: Inspecting existing credential store metadata
- **WHEN** inspecting diagnostics for an existing, populated credential store file
- **THEN** returns a diagnostic record with `exists: true`, byte length, and available credential keys

