# Design

## Context

Stage 0 of the Canon Clerk evaluation cascade determines which repository canons are activated by a set of modified or prospective files at zero token cost.

With `@canon-clerk/core`'s streaming `queryCanons` engine in place, `@canon-clerk/cli` acts as an external presentation adapter. It translates CLI operands, standard input streams, and flags into domain query inputs, and formats relational activation tuples into human-readable terminal trees or canonical JSON records.

## Goals / Non-Goals

**Goals:**
- Implement `canon-clerk check-triggers` as the single authoritative Stage 0 CLI subcommand with no aliases.
- Provide symmetrical input channels for targets and canons: concrete files, globs, standard input (`-`), and workspace-wide evaluation (`--all`).
- Enforce strict input guardrails: mutual exclusivity on standard input streams, and usage error exit code 2 on naked invocations.
- Implement tiered ignore controls adhering to `.gitignore` semantics, ensuring explicit concrete targets take precedence over default noise ignores.
- Present human-readable `stylish` tree reports displaying canon scope, triggering target paths, and matched trigger patterns.
- Emit a canonical JSON array under `--json` capturing full 3-plane pattern attribution.
- Provide predicate mode (`--quiet` / `-q`) that short-circuits on the first streamed activation tuple and returns boolean exit status.

**Non-Goals:**
- Executing Stage 1 Screener or Stage 2 Deep Auditor LLM evaluations (pass/fail verdicts on code changes).
- Shelling out to Git or any specific VCS binary.
- Subcommand aliases (`match`, `query`).

## Decisions

### Decision 1: Git-Inspired Cognitive Anchor (`check-triggers` with No Aliases)
- **Choice:** Name the subcommand `check-triggers` and provide zero aliases (`match` and `query` are omitted).
- **Precedent & Salience:** Grounded directly in `git check-ignore` and `git check-attr`. In Git, `git check-ignore <path>` checks `.gitignore` patterns against target files; in Canon Clerk, `canon-clerk check-triggers <path>` checks canon `triggers:` frontmatter against target files at zero token cost.
- **Rationale:** AI coding agents and human contributors have strong pre-trained latent priors for `git check-*` commands. Naming the command `check-triggers` immediately communicates that this is a fast, deterministic, read-only pattern check. It prevents confusion with canon syntax validation (`lint`) and deep compliance auditing (`audit`). Omitting aliases eliminates documentation fragmentation and cognitive overhead.
- **Alternatives Considered:** Bare `match` (rejected as overly mechanism-focused and ambiguous), `query` (rejected as overly generic and reserved for future catalog/tag searches).

### Decision 2: VCS-Agnostic UNIX Composability via Explicit Stdin (`-`)
- **Choice:** Support `-` as a target operand to read line-delimited file paths from standard input, and `--canon -` to read canon paths from standard input. Reject implicit standard input consumption without `-`.
- **Rationale:** Canon Clerk is a canon enforcement gate, not a VCS wrapper. Piping paths from standard input enables seamless composition with any version control toolchain (`git diff --name-only | canon-clerk check-triggers -`, `jj diff --name-only`, `hg status`, etc.). Adhering to canon `cli-stdin-consumption-must-require-explicit-operands`, reading stdin only when `-` is explicitly supplied prevents commands from hanging in CI runners.

### Decision 3: Symmetrical Bipartite Inputs & Single-Canon Inversion
- **Choice:** Model the input space as two symmetrical dimensions: Target Files (subject code) and Canon Files (rule definitions).
- **Single-Canon Inversion:** When `--canon <path>` is supplied without target operands, targets naturally default to all files in scope (`**/*`). The CLI sets `targetQuery: '**/*'`, relying on `queryCanons`'s zero-I/O scope screening to discard out-of-scope targets.
- **Mutual Exclusivity:** Standard input can be directed to targets (`-`) or canons (`--canon -`), but cannot be supplied to both simultaneously (exits with status 2).

### Decision 4: Naked Invocation Guardrails
- **Choice:** Invoking `canon-clerk check-triggers` naked without positional targets, `-`, `--all`, or `--canon` terminates with exit code 2 (Usage Error) and prints a remediation hint to `stderr`.
- **Rationale:** Evaluating the full Cartesian product of an entire repository unprompted (e.g. 50,000 files $\times$ 100 canons) is computationally heavy and noisy. Requiring an explicit `--all` flag ensures intentional execution while protecting users from accidental runaway evaluation.

### Decision 5: Three-Plane Pattern Attribution in Canonical JSON
- **Choice:** Emit a flat array of match records under `--format json` or `--json` with full 3-plane pattern attribution:
  1. Plane 1 (Target Selection): `matchedTargetPatterns` (patterns that selected the target).
  2. Plane 2 (Canon Selection): `matchedCanonPatterns` (patterns that discovered the canon).
  3. Plane 3 (Domain Binding): `matchedTriggers` (frontmatter triggers that activated the canon).
- **Target Path Semantics:** Each matched target record includes `targetPath` (workspace-relative path), `targetScopeRelativePath` (scope-relative path against which triggers evaluate), and `targetRelativePath` (alias to `targetScopeRelativePath` for backward compatibility).
- **Empty Query Rule:** In non-quiet modes, zero matches output `[]` with exit status 0 per canon `cli-queries-must-exit-zero-on-empty-results`.

### Decision 6: Short-Circuiting Predicate Evaluation in Quiet Mode
- **Choice:** When `--quiet` (`-q`) is enabled, stdout is suppressed, and execution terminates on the very first match tuple yielded by `queryCanons`, exiting with code 0. If the generator completes with zero matches, the process exits with code 1.
- **Rationale:** Predicate evaluation in shell scripts (`if canon-clerk check-triggers src/foo.ts -q; then ...`) requires only a boolean check. Short-circuiting avoids evaluating remaining candidates, providing maximal performance.

### Decision 7: Tiered Ignore Hierarchy with Explicit Concrete Precedence
- **Choice:** Support shared ignores (`--ignore`, `--default-ignores`, `--no-default-ignores`) alongside granular overrides (`--target-ignore`, `--canon-ignore`, `--no-default-target-ignores`, `--no-default-canon-ignores`). Concrete literal paths bypass default noise ignores (explicit target precedence).
- **Rationale:** Matches core engine capabilities and ensures contributors can test canons or targets located in typically ignored paths (e.g. fixtures or generated files) without disabling noise protection globally.

## Risks / Trade-offs

- **[Risk: Large workspace Cartesian evaluation under `--all`]** → **Mitigation:** Naked invocation requires explicit `--all` flag; `queryCanons` applies zero-I/O scope screening before checking trigger globs.
- **[Risk: Stdin stream deadlock]** → **Mitigation:** Strictly require explicit `-` operand; enforce mutual exclusivity between target and canon stdin streams with exit code 2.
