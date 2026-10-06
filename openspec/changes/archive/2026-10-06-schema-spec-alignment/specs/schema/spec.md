# Spec Delta: schema

## MODIFIED Requirements

### Requirement: Canon Domain Entity Schema
The system SHALL represent parsed canons using a unified, flat `Canon` domain entity containing required machine identifiers (`id`), human-readable titles (`title`), deterministic path activation triggers (`triggers`), state preconditions (`exists: readonly string[]`), context exhibits with token optionality (`inspect: readonly InspectPlane[]`), classification tags (`tags`), grounding file references (`references`), cognitive tetrad attributes, and file provenance metadata.

#### Scenario: Fully specified canon representation
- **WHEN** a canon provides complete frontmatter and markdown sections (What, Exception, Rationale, Remediation)
- **THEN** the system produces a normalized `Canon` entity with all metadata and cognitive directive attributes exposed as direct properties on the entity

#### Scenario: Fully specified canon with preconditions and inspect riders
- **WHEN** a canon provides frontmatter with explicit `exists` preconditions and `inspect` tokens containing `?` riders
- **THEN** the system produces a normalized `Canon` entity exposing `exists` as an array of patterns and `inspect` as an array of `InspectPlane` objects

#### Scenario: Inspect plane optionality
- **WHEN** an inspect token carries the trailing `?` modifier (e.g. `pr_title?`)
- **THEN** the parsed `InspectPlane` has `optional: true`, whereas bare tokens have `optional: false`

### Requirement: Deterministic Metadata Derivation
The system SHALL derive missing canon metadata per SPEC.md Section 4.2:
1. `id`: Normalized kebab-case frontmatter `id`, falling back to the relative file path stem.
2. `title`: Verbatim frontmatter `title`, falling back to the first Markdown heading, then Title Case of `id`.
3. `triggers`: Frontmatter `triggers` list, falling back to `["**/*"]` for root canons or `<scope>/**` for scoped canons.
4. `exists`: Frontmatter `exists` list (coercing scalar string to list), falling back to `[]`.
5. `inspect`: Frontmatter `inspect` list (parsing `?` riders), falling back to `["diff", "pr_title?", "pr_body?"]`.
6. `tags`: Frontmatter `tags` list (coercing scalar string to list), falling back to `[]`.
7. `references`: Frontmatter `references` list, falling back to `[]`.
8. `invariant`: Primary body invariant statement, falling back to derived `title` for empty/heading-only canons.

#### Scenario: Deriving metadata from bare markdown canon
- **WHEN** a canon markdown file contains no frontmatter and begins with `# Prs Must Include Tests`
- **THEN** the system derives `id` from the file stem, `title` as `"Prs Must Include Tests"`, `triggers` as `["**/*"]`, `exists` as `[]`, `inspect` as `[{ token: 'diff', optional: false }, { token: 'pr_title', optional: true }, { token: 'pr_body', optional: true }]`, `tags` as `[]`, and `references` as `[]`

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

#### Scenario: Default exists preconditions
- **WHEN** a canon omits the `exists` frontmatter field
- **THEN** the derived `exists` property defaults to an empty array `[]`

#### Scenario: Scalar coercion for exists
- **WHEN** a canon specifies `exists: "package.json"` as a scalar string
- **THEN** the system normalizes `exists` to `["package.json"]`

#### Scenario: Default inspect planes with optional riders
- **WHEN** a canon omits the `inspect` frontmatter field
- **THEN** `inspect` derives `[{ token: 'diff', optional: false }, { token: 'pr_title', optional: true }, { token: 'pr_body', optional: true }]`

## ADDED Requirements

### Requirement: Scope Containment and Subordination
The system SHALL enforce strict monorepo package isolation for scoped canons located in `<scope>/.canons/`. All declared paths and glob patterns in `triggers:`, `exists:`, and `references:` SHALL be evaluated strictly relative to `<scope>/`. Any pattern attempting directory traversal outside `<scope>/` (such as `../`) SHALL be rejected during derivation and validation.

#### Scenario: Automatically scoping exists preconditions in scoped canons
- **WHEN** a scoped canon in `packages/ui/.canons/rule.md` specifies `exists: "package.json"`
- **THEN** the system scopes the precondition to `packages/ui/package.json`

#### Scenario: Rejecting directory traversal escaping scope
- **WHEN** a scoped canon specifies a path attempting directory traversal outside its scope (e.g. `../packages/core/src`)
- **THEN** the system rejects the definition with a scope containment error
