# Design

## Context

`@canon-clerk/schema` currently contains only an unaligned placeholder interface (`Canon`), lacking alignment with `SPEC.md` Section 4 and missing runtime normalization logic. Furthermore, `packages/schema/.canons/schema-must-be-pure-and-free-of-io.md` strictly mandates that `@canon-clerk/schema` must be 100% pure, containing no Node.js I/O or filesystem calls. See `proposal.md` for overall motivation.

## Goals / Non-Goals

**Goals:**
- Provide complete TypeScript types for metadata, cognitive directives (What, When, Why, How), and the unified flat `Canon` entity.
- Implement pure normalization functions (`parseCanon(content, options)`) that take in-memory markdown strings and optional path context to derive fully resolved `Canon` domain entities.
- Implement Section 4.2 derivation rules for `id`, `title`, `triggers`, `inspect`, `tags`, and `references`.
- Parse cognitive directives (`Exception`, `Rationale`, `Remediation`) into structured properties directly on `Canon`.

**Non-Goals:**
- Filesystem scanning (`node:fs`), directory walking, or glob resolution against files on disk (managed by `@canon-clerk/core`).
- Git diff parsing or PR API integration (managed by `@canon-clerk/core` and `@canon-clerk/action`).

## Decisions

### 1. Pure In-Memory Parser Boundary
* **Decision:** Keep all parsing and normalization functions strictly pure within `@canon-clerk/schema`. Consumers in `@canon-clerk/core` will read files from disk or git and pass raw string contents and relative paths into `@canon-clerk/schema`.
* **Rationale:** Complies with `schema-must-be-pure-and-free-of-io.md` and ISO/IEC 25010 portability across Node, edge, and browser runtimes.
* **Alternatives Considered:** Embedding `loadCanon(filePath)` in `@canon-clerk/schema`. Rejected because coupling schema to filesystem I/O violates internal dogfood canons.

### 2. Frontmatter Parsing via Pure YAML Parser
* **Decision:** Use the zero-dependency, pure JavaScript `yaml` package (already present in the monorepo lockfile) to parse YAML frontmatter between `---` boundaries.
* **Rationale:** Robust handling of YAML mappings, scalar coercion, comments, and syntax diagnostics without native bindings or I/O.
* **Alternatives Considered:** Custom regex frontmatter parsing. Rejected because hand-rolled regex easily breaks on multi-line lists, nested objects, and comments.

### 3. Unified Flat Canon Domain Entity
* **Decision:** Represent the parsed canon as a unified flat `Canon` domain entity without arbitrary enclosures:
  * Identity & Metadata: `id`, `title`, `triggers`, `inspect`, `tags`, `references`.
  * Cognitive Tetrad: `invariant` (What), `exceptions: string[]` (When), `rationale` (Why), `remediation` (How).
  * Raw Text & Provenance: `rawBody`, `rawFrontmatter` (`Record<string, unknown> | undefined`), `rawContent`, `filePath`, `scope`.
* **Rationale:** Eliminates arbitrary stratification between metadata and body directives. Enables downstream consumers (CLI, Check Runs, Deep Auditor prompt builders) to access attributes directly (`canon.id`, `canon.invariant`, `canon.triggers`) without nested indirection. Discrete exceptions are collected into an array for independent logical OR evaluation per SPEC.md Section 5.

### 4. Metadata Derivation Precedence Matrix
* **Decision:** Implement exact resolution hierarchy matching `SPEC.md` Section 4.2:
  * `id`: Frontmatter `id` (normalized to kebab-case) $\rightarrow$ relative path slug omitting `.md`.
  * `title`: Frontmatter `title` $\rightarrow$ first `#` or `##` heading $\rightarrow$ Title Case of `id`.
  * `triggers`: Frontmatter `triggers` $\rightarrow$ `["<scope>/**"]` if scoped $\rightarrow$ `["**/*"]` if global.
  * `inspect`: Frontmatter `inspect` $\rightarrow$ `["diff", "pr_title", "pr_body"]`.
  * `tags`: Frontmatter `tags` (coercing scalar string to array) $\rightarrow$ `[]`.
  * `references`: Frontmatter `references` $\rightarrow$ `[]`.
  * `invariant`: Body text outside directives $\rightarrow$ derived `title` (supporting 0-byte and heading-only canons).

### 5. Two-Stage Lexer and Parser Architecture
* **Decision:** Separate lexical analysis (`tokenizeCanon`) from syntactic parsing and normalization (`parseCanon`):
  * **Lexer (`tokenizeCanon`):** Emits an ordered stream of typed lexical tokens (`CanonToken`) tracking 1-indexed source line numbers, raw source text, and token-specific properties (code blocks, frontmatter, headings, directives, text, blank lines). Directives preserve raw labels and trimmed values (including empty values for whitespace-only directives).
  * **Parser (`parseCanon`):** Consumes the token stream to assemble the normalized `Canon` domain entity, resolving fallbacks, defaults, and multi-line continuations.
  * **Linter Enablement:** Downstream linters can inspect the token stream directly to detect style issues (such as empty directives or duplicate clauses) with exact line numbers, without needing to re-scan raw text.
* **Rationale:** Adheres to single-responsibility and compiler design best practices. Decouples tolerant domain normalization from strict diagnostic linting.

## Risks / Trade-offs

- **[Malformed Frontmatter]** $\rightarrow$ Parser validates frontmatter types and emits clean descriptive parse errors if YAML syntax is invalid or frontmatter is not a mapping.
- **[Directive Formatting Variations]** $\rightarrow$ Canon authors may use `**Remediation:**`, `Remediation:`, or markdown headers. The parser normalizes case-insensitive directive prefixes while preserving body markdown formatting.
