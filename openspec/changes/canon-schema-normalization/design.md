# Design

## Context

`@canon-clerk/schema` currently contains only an unaligned placeholder interface (`Canon`), lacking alignment with `SPEC.md` Section 4 and missing runtime normalization logic. Furthermore, `packages/schema/.canons/schema-must-be-pure-and-free-of-io.md` strictly mandates that `@canon-clerk/schema` must be 100% pure, containing no Node.js I/O or filesystem calls. See `proposal.md` for overall motivation.

## Goals / Non-Goals

**Goals:**
- Provide complete TypeScript types for frontmatter, derived metadata, cognitive sections (What, When, Why, How), and the unified `Canon` entity.
- Implement pure normalization functions (`parseCanon(content, options)`) that take in-memory markdown strings and optional path context to derive fully resolved `Canon` domain entities.
- Implement Section 4.2 derivation rules for `id`, `title`, `triggers`, `inspect`, `tags`, and `references`.
- Parse cognitive directives (`Exception`, `Rationale`, `Remediation`) into structured AST nodes.

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

### 3. Progressive AST Decomposition for Cognitive Tetrad
* **Decision:** Parse the markdown body into structured sections:
  * `invariant`: The primary invariant statement (What).
  * `exception`: Permissible deviation conditions (`Exception: ...`).
  * `rationale`: Precedent reasoning (`Rationale: ...`).
  * `remediation`: Actionable instructions for contributors (`Remediation: ...` or `**Remediation:** ...`).
* **Rationale:** Reflects the living `SPEC.md` What/When/Why/How tetrad. Enables downstream Deep Auditor and CLI formatters to selectively access directives without ad-hoc string searching.

### 4. Metadata Derivation Precedence Matrix
* **Decision:** Implement exact resolution hierarchy matching `SPEC.md` Section 4.2:
  * `id`: Frontmatter `id` (normalized to kebab-case) $\rightarrow$ relative path slug omitting `.md`.
  * `title`: Frontmatter `title` $\rightarrow$ first `#` or `##` heading $\rightarrow$ Title Case of `id`.
  * `triggers`: Frontmatter `triggers` $\rightarrow$ `["<scope>/**"]` if scoped $\rightarrow$ `["**/*"]` if global.
  * `inspect`: Frontmatter `inspect` $\rightarrow$ `["diff", "pr_title", "pr_body"]`.
  * `tags`: Frontmatter `tags` (coercing scalar string to array) $\rightarrow$ `[]`.
  * `references`: Frontmatter `references` $\rightarrow$ `[]`.

## Risks / Trade-offs

- **[Malformed Frontmatter]** $\rightarrow$ Parser validates frontmatter types and emits clean descriptive parse errors if YAML syntax is invalid or frontmatter is not a mapping.
- **[Directive Formatting Variations]** $\rightarrow$ Canon authors may use `**Remediation:**`, `Remediation:`, or markdown headers. The parser normalizes case-insensitive directive prefixes while preserving body markdown formatting.
