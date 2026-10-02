# path-filter/triggers Specification

## Purpose

Defines scope-relative trigger pattern evaluation in `@canon-clerk/core`, determining whether an in-scope target file activates a canon based on declared path triggers.

## Requirements

### Requirement: Default and Wildcard Trigger Activation
The system SHALL activate evaluation whenever trigger patterns are omitted, empty, or contain a global wildcard matching all files.

#### Scenario: Omitted or empty trigger patterns
- **WHEN** evaluating a target path with undefined or empty triggers
- **THEN** returns triggered true with matchedTriggers containing '**/*'

#### Scenario: Universal wildcard trigger
- **WHEN** evaluating a target path where triggers contains '**/*' or '**'
- **THEN** returns triggered true with matchedTriggers containing '**/*'

### Requirement: Scope-Relative Pattern and Dotfile Matching
The system SHALL evaluate scope-relative target paths against declared trigger globs using glob matching semantics with dotfile awareness, returning all matching patterns.

#### Scenario: Matching specific paths and extensions
- **WHEN** evaluating a target path matching one or more declared trigger globs
- **THEN** returns triggered true and lists all matching trigger patterns in matchedTriggers

#### Scenario: Non-matching target path
- **WHEN** evaluating a target path that matches none of the declared trigger patterns
- **THEN** returns triggered false and an empty matchedTriggers list

#### Scenario: Dotfile path matching
- **WHEN** evaluating a dotfile target path against a wildcard or dotfile trigger pattern
- **THEN** matches successfully with dotfile visibility enabled
