# Proposal

## Why

Stage 0 of the Canon Clerk evaluation cascade determines which repository canons are activated by a set of modified or prospective files at zero token cost. While `@canon-clerk/core` provides the streaming query engine (`queryCanons`), human contributors, coding agents, and CI pipelines lack an ergonomic, salient command-line interface to answer:
- "What canons govern this file I'm about to edit?" (1:N)
- "What canons govern these specific files?" (N:M)
- "What canons govern files modified in my workspace or changelist?" (N:M via standard input)

Drawing on the proven cognitive precedent of `git check-ignore` and `git check-attr`, `@canon-clerk/cli` needs a dedicated `check-triggers` subcommand to test target file paths against canon frontmatter triggers and package scopes deterministically, providing an unmistakable latent anchor for AI coding agents and human developers alike.

## What Changes

- Implement the `check-triggers` subcommand in `@canon-clerk/cli` (with no aliases) delegating domain evaluation directly to `@canon-clerk/core`'s `queryCanons`.
- Support symmetrical input channels: positional targets, globs, standard input (`-`), and workspace-wide evaluation (`--all`), alongside canon selection (`--canon`, `--canons`, `--canon-glob`, and `--canon -`).
- Enforce strict input guardrails: disallow simultaneous stdin on targets and canons, and reject naked invocations with usage exit code 2 and actionable remediation hints.
- Implement tiered ignore controls: shared defaults (`--ignore`, `--no-default-ignores`) and granular overrides (`--target-ignore`, `--canon-ignore`, `--no-default-target-ignores`, `--no-default-canon-ignores`), with explicit target precedence.
- Implement `stylish` terminal presentation grouped by canon, displaying scope and triggering files with matched trigger patterns.
- Implement canonical `json` projection emitting a structured array with 3-plane pattern attribution (`matchedCanonPatterns`, `matchedTargetPatterns`, `matchedTriggers`).
- Implement predicate mode (`--quiet` / `-q`), suppressing stdout and short-circuiting on the first discovered match with boolean exit codes (0 for triggered, 1 for none).
- Update the root CLI router and help screen in `canon-clerk` to document `check-triggers`.

## Capabilities

### New Capabilities
- `cli/check-triggers`: Stage 0 canon trigger evaluation subcommand, target and canon operand resolution, standard input piping, terminal tree presentation, canonical JSON attribution, and predicate mode exit codes.

## Impact

- `@canon-clerk/cli`: Exposes Stage 0 evaluation to terminal workflows, git hooks, and agent tool execution.
- Contributor and AI Agent Workflows: Enables zero-token, instant inspection of which canons govern modified or prospective files (e.g. `git diff --name-only | canon-clerk check-triggers -`).
- CI Pipelines: Enables fast conditional gating in PR checks and pre-commit scripts.
