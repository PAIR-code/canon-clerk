# Spec Delta

## Purpose

Defines the core evaluation engine contracts, canonical canon data representations, AST interfaces, and deterministic metadata derivation rules governing Canon Clerk.

## ADDED Requirements

### Requirement: Canon Domain Entity Schema
The system SHALL represent parsed canons using a normalized `Canon` domain entity containing required machine identifiers (`id`), human-readable titles (`title`), deterministic path activation triggers (`triggers`), Deep Auditor inspection contexts (`inspect`), classification tags (`tags`), persistent grounding file references (`references`), and structured section ASTs.

#### Scenario: Fully specified canon representation
- **WHEN** a canon provides complete frontmatter and markdown sections (What, Exception, Rationale, Remediation)
- **THEN** the system produces a normalized `Canon` entity with all frontmatter attributes typed and sections parsed into structured AST nodes

### Requirement: Deterministic Metadata Derivation
The system SHALL derive missing canon metadata according to the deterministic hierarchy defined in SPEC.md Section 4.2:
1. `id`: Normalized kebab-case frontmatter `id`, falling back to the relative file path stem omitting the `.md` extension.
2. `title`: Verbatim frontmatter `title`, falling back to the first Markdown heading (`#` or `##`), falling back to Title Case of the derived `id`.
3. `triggers`: Frontmatter `triggers` list, falling back to `["**/*"]` for root canons or `<scope>/**` for scoped canons.
4. `inspect`: Frontmatter `inspect` list, falling back to `["diff", "pr_title", "pr_body"]`.
5. `tags`: Frontmatter `tags` list (coercing scalar string to a single-element list), falling back to `[]`.
6. `references`: Frontmatter `references` list, falling back to `[]`.
7. `invariant`: Primary body invariant statement, falling back to the derived `title` when the body contains zero invariant text.

#### Scenario: Deriving metadata from bare markdown canon
- **WHEN** a canon markdown file contains no frontmatter and begins with `# Prs Must Include Tests`
- **THEN** the system derives `id` from the file stem, `title` as `"Prs Must Include Tests"`, `triggers` as `["**/*"]`, `inspect` as `["diff", "pr_title", "pr_body"]`, `tags` as `[]`, and `references` as `[]`

#### Scenario: Deriving invariant from filename for zero-byte canon
- **WHEN** a canon markdown file is completely empty (0 bytes) with path `.canons/all-caps-spec-must-refer-to-spec-md.md`
- **THEN** the system derives `id` from the file stem, `title` as `"All Caps Spec Must Refer To Spec Md"`, and `invariant` as `"All Caps Spec Must Refer To Spec Md"`

#### Scenario: Deriving invariant from heading in heading-only canon
- **WHEN** a canon markdown file contains only a single heading `# PRs Must Include Tests` with zero body text
- **THEN** the system derives `title` and `invariant` as `"PRs Must Include Tests"`

#### Scenario: Coercing scalar frontmatter tags
- **WHEN** a canon frontmatter specifies `tags: internal` as a scalar string
- **THEN** the system normalizes `tags` to the array `["internal"]`

#### Scenario: Scoped canon trigger boundary
- **WHEN** a canon without explicit triggers resides at `packages/ui/.canons/button-must-have-aria.md`
- **THEN** the system scopes the default triggers to `["packages/ui/**"]`

### Requirement: Pure Parsing and Normalization
The canon parsing and normalization functions in `@canon-clerk/schema` SHALL be pure functions that operate exclusively on in-memory strings and options, with zero dependency on Node.js I/O or filesystem modules.

#### Scenario: In-memory parsing without filesystem access
- **WHEN** raw markdown text and relative file path metadata are passed to the parser
- **THEN** the system returns a normalized `Canon` entity without performing filesystem or network operations

### Requirement: Cognitive Directives and Section AST
The parser SHALL parse the canon body into structured sections, separating the primary invariant statement (What) from recognized directives: `Exception` (When), `Rationale` (Why), and `Remediation` (How). Discrete `Exception` clauses SHALL be collected into a normalized `exceptions: string[]` array containing zero or more elements, evaluated as independent logical ORs.

#### Scenario: Parsing the complete cognitive tetrad
- **WHEN** a canon body contains an invariant statement, `Exception:` clause, `Rationale:` clause, and `**Remediation:**` clause
- **THEN** the parsed AST categorizes each directive into distinct, typed fields on the canon entity, with `exceptions` containing the single extracted exception clause

#### Scenario: Parsing multiple discrete exception clauses
- **WHEN** a canon body declares multiple discrete `Exception:` directives
- **THEN** the parsed AST collects each discrete clause into `exceptions: string[]` preserving declaration order
