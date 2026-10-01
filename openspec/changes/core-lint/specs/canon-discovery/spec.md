# Spec Delta

## Purpose

Defines filesystem discovery and querying mechanisms for locating canon markdown files across repository workspaces and package boundaries. Implemented in `@canon-clerk/core` (`src/discovery.ts`).

## ADDED Requirements

### Requirement: Workspace Canon File Discovery
The system SHALL discover canon markdown files residing in `.canons/` directories across the workspace root and package subdirectories. Discovery MUST exclude default noise directories (`node_modules`, `dist`, `.bare`, `.git`) unless disabled, support custom exclusion patterns, and support target path filtering with explicit target precedence, returning normalized relative workspace paths.

#### Scenario: Discovering canons across workspace and packages
- **WHEN** discovering canons in a workspace containing root `.canons/` and package-level `packages/*/.canons/`
- **THEN** all `.md` files within any `.canons/` directory are returned as normalized relative paths

#### Scenario: Ignoring non-canon and default excluded directories
- **WHEN** discovering canons in a workspace containing `node_modules`, `.git`, `dist`, or markdown files outside `.canons/`
- **THEN** non-canon markdown files and excluded directories are omitted from discovery results

#### Scenario: Custom directory exclusions
- **WHEN** discovering canons with custom exclusion paths or patterns specified in options
- **THEN** matching directories and files are excluded from discovery

#### Scenario: Disabling default exclusions
- **WHEN** discovering canons with default exclusions explicitly disabled
- **THEN** directories normally pruned by default are traversed and evaluated

#### Scenario: Explicit target precedence over default exclusions
- **WHEN** discovering canons with an explicit target pointing to a canon file inside a default-excluded directory
- **THEN** the explicitly targeted canon file is included in discovery results

#### Scenario: Filtering discovery by target paths
- **WHEN** discovering canons with targets specifying a specific package directory or single canon file
- **THEN** only canon files matching the specified targets are returned

#### Scenario: Normalizing relative POSIX paths
- **WHEN** discovering canon files on any host operating system
- **THEN** all returned file paths are normalized to POSIX relative paths using forward slashes
