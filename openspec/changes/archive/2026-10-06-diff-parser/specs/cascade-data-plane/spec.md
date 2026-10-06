# Spec Delta

## ADDED Requirements
### Requirement: Pure In-Memory and Streaming Unified Diff Parsing
The system SHALL provide unified diff parsers that convert Git patch streams into `FileArtifact` domain entities, supporting both synchronous in-memory collection and asynchronous stream generation.

#### Scenario: Streaming file artifacts across chunk boundaries
- **WHEN** consuming an asynchronous chunked stream containing multiple file diffs
- **THEN** yields each `FileArtifact` incrementally as file demarcation boundaries or stream end are encountered

#### Scenario: Parsing added file in unified diff
- **WHEN** parsing a diff stream containing a new file creation with hunk lines
- **THEN** produces an artifact with status `'added'`, computed added line statistics, patch content, and default content omission reason

#### Scenario: Parsing deleted file in unified diff
- **WHEN** parsing a diff stream containing a deleted file with hunk lines
- **THEN** produces an artifact with status `'deleted'`, computed deleted line statistics, patch content, and deleted content omission reason

#### Scenario: Parsing modified file in unified diff
- **WHEN** parsing a diff stream containing edits to an existing file
- **THEN** produces an artifact with status `'modified'`, accurate added and deleted line counts, and hunk patch text

#### Scenario: Parsing renamed file without hunks
- **WHEN** parsing a diff stream containing a 100% similarity rename without hunks
- **THEN** produces an artifact with status `'renamed'`, previous path populated, undefined patch, and patch omission reason set to `'unchanged'`

#### Scenario: Parsing renamed file with modifications
- **WHEN** parsing a diff stream containing a rename with hunk modifications
- **THEN** produces an artifact with status `'renamed'`, previous path populated, and hunk patch text populated

#### Scenario: Parsing copied file in unified diff
- **WHEN** parsing a diff stream containing a copy directive
- **THEN** produces an artifact with status `'copied'`, original source path recorded in previous path, and patch text

#### Scenario: Parsing binary file modification
- **WHEN** parsing a diff stream containing binary file markers
- **THEN** produces an artifact with zero line counts and both patch and content omission reasons set to `'binary'`

#### Scenario: Parsing single-line hunk coordinates
- **WHEN** parsing hunk headers omitting line count coordinates
- **THEN** accurately parses coordinates and counts hunk line changes

#### Scenario: Parsing quoted file paths with whitespace
- **WHEN** parsing Git diff headers containing quoted paths with spaces or escaped characters
- **THEN** unquotes and unescapes the file paths into standard relative POSIX paths

#### Scenario: Parsing empty or whitespace-only diff streams
- **WHEN** parsing a diff stream that is empty or consists solely of whitespace
- **THEN** returns an empty file artifact map or terminates stream without emissions
