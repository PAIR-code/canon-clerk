# Design: Frontmatter Schema & Metadata Lint Rules

## Context
Following the implementation of the static linting framework in #116, `@canon-clerk/schema` contains `RuleContext`, `lintCanon()`, and basic tokenization, but zero domain lint rules. Canons in Canon Clerk rely on frontmatter YAML mappings for metadata (`id`, `title`, `triggers`, `inspect`, `tags`, `references`), which must adhere to SPEC.md Section 4. Frontmatter rules must execute deterministically against `RuleContext` without filesystem or network I/O, complying with `schema-must-be-pure-and-free-of-io.md`.

## Goals / Non-Goals
**Goals:**
- Implement 6 pure lint rules: `valid-yaml-frontmatter`, `no-unrecognized-keys`, `valid-frontmatter-types`, `id-matches-filename`, `no-negated-file-stems`, and `no-negated-ids`.
- Leverage YAML CST (`frontmatterDoc`, `LineCounter`) in `RuleContext` to emit exact 1-indexed source line and column numbers for key and syntax diagnostics.
- Ensure fault-tolerant evaluation that does not crash when encountering malformed delimiters, unclosed frontmatter, or non-object frontmatter.
- Register all rules in a dedicated `FRONTMATTER_RULES` catalog and aggregate `DEFAULT_RULES` export in `@canon-clerk/schema`.

**Non-Goals:**
- Validating that referenced files in `references:` exist on disk (deferred to `@canon-clerk/core` to preserve pure in-memory contract).
- Validating markdown body structure or directive semantics (tracked in #118 and #119).
- CLI runner or formatting reporters (handled in `@canon-clerk/cli`).

## Decisions
### 1. CST Node Position Resolution
- **Decision:** Use `yaml` package CST nodes (`Pair.key` range) coupled with `frontmatterLineCounter.linePos` to resolve 1-indexed line and column coordinates for frontmatter keys.
- **Rationale:** Linters must point authors to the exact character coordinates of syntax and schema errors, rather than defaulting to line 1.
- **Offset Handling:** Because frontmatter YAML starts on line 2 (line 1 being the opening `---`), CST line numbers are offset by +1 to correspond to source file lines.

### 2. Delimiter & Parsing Fault Tolerance
- **Decision:** `valid-yaml-frontmatter` inspects `context.rawContent` for opening and terminating `---` delimiters directly, while checking `context.frontmatterDoc.errors` for YAML parsing errors.
- **Rationale:** If a file has an unclosed `---`, `RuleContext` or `valid-yaml-frontmatter` handles it gracefully without throwing unhandled exceptions, producing an actionable diagnostic with `remediation`.

### 3. Modular Rule Directory Structure
- **Decision:** Place rule implementations under `packages/schema/src/rules/frontmatter/`, grouping related rules and exporting individual rules as well as a `FRONTMATTER_RULES` array.
- **Rationale:** Keeps individual rule files cohesive, single-responsibility, and easy to unit test. Follow-on issues (#118 for directives, #119 for markdown) will follow the same pattern (`rules/directives/`, `rules/structure/`).

### 4. Negative Modal Verb Detection Pattern
- **Decision:** Flag `-must-not-` (and boundary variants like `^must-not-` or `-must-not$`) in file stems and `must-not` in explicit IDs using word/hyphen boundary matching.
- **Rationale:** Canon authoring standards encourage affirmative actions (`must-omit`) or categorical prohibition (`...-are-forbidden`). A regex matching kebab-case boundaries (`/(?:^|-)must-not(?:-|$)/`) avoids false positives on unrelated substrings while accurately catching negative modals.

## Risks / Trade-offs
- **Malformed YAML Cascades:** Malformed YAML prevents CST creation, meaning schema checks (`no-unrecognized-keys`, `valid-frontmatter-types`) cannot run.
  - *Mitigation:* Rules check if `context.frontmatterDoc` has parse errors or is undefined, gracefully returning `[]` and allowing `valid-yaml-frontmatter` to report the syntax defect.
