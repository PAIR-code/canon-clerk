# Canon Format Specification

**Document Status:** Living Standard  
**Authority:** Normative definition governing Canon Clerk reference implementation parser, screener, and auditor implementations.

---

## 1. Scope & Conformance

### 1.1 Purpose
This specification defines the syntax, schema, progressive disclosure tiers, and metadata derivation rules for **project canons**—declarative, natural language architectural invariants stored in repository `.canons/` directories.

### 1.2 Conformance Terminology
The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in BCP 14 [[RFC 2119](https://www.rfc-editor.org/rfc/rfc2119)] [[RFC 8174](https://www.rfc-editor.org/rfc/rfc8174)] when, and only when, they appear in all capitals, as shown here.

### 1.3 Core Principles
1. **Zero Barrier to Entry:** A single sentence in a plain text Markdown file MUST be treated as a fully valid, enforceable canon.
2. **Progressive Disclosure:** Advanced optimizations (deterministic path filtering, custom inspection scopes, and structured rubrics) are strictly OPTIONAL.
3. **Clarity of Action:** The specification cleanly partitions contributor-directed guidance (blocking violations) from engine-directed synthesis (non-blocking enhancements).

---

## 2. Document Model & Syntax

### 2.1 File Encoding & Format
* A canon file MUST be a valid UTF-8 encoded text file.
* A canon file MUST use the `.md` file extension.
* The body of a canon file is interpreted as CommonMark / GitHub Flavored Markdown (GFM).

### 2.2 Frontmatter Block
* A canon MAY begin with an optional YAML frontmatter block.
* If present, the frontmatter block MUST open with three hyphens (`---`) on the first line of the file and terminate with three hyphens (`---`) on a subsequent line.
* If present, the content between delimiters MUST be valid YAML mapping syntax.
* If frontmatter is omitted or empty, the entire file content is treated as the Markdown body, and all metadata fields MUST be derived according to the fallback rules in [Section 4](#4-metadata-schema--derivation-rules).

---

## 3. File Discovery & Scoping

### 3.1 Canon Discovery 
By default, canon discovery SHOULD search for all Markdown files matching the pattern:
```text
**/.canons/**/*.md
```
Implementations MAY allow or use a different pattern, for example to allow project-specific configuration. 

### 3.2 Global Canons
Canons residing in the repository root's `.canons/` directory are **global** and apply across the entire repository.

### 3.3 Scoped Canons (Monorepo Packages)
Canons MAY reside within subdirectories, such as monorepo packages or services:
```text
packages/ui/.canons/prs-must-verify-accessibility.md
services/auth/.canons/tokens-must-expire-promptly.md
```
**Scoping Rule:** A canon located in `<scope>/.canons/` automatically inherits an implicit path boundary of `<scope>/**`. Implementations MUST NOT evaluate a scoped canon against a pull request if zero modified files reside within `<scope>/`.

---

## 4. Metadata Schema & Derivation Rules

### 4.1 Schema Definitions

| Field | Type | Required? | Default / Resolution Order | Description |
| :--- | :--- | :--- | :--- | :--- |
| **`id`** | `string` | No | 1. Frontmatter `id:`<br>2. Relative file stem / slug | Machine identifier used in Check Runs, CLI output, and state tracking. |
| **`title`** | `string` | No | 1. Frontmatter `title:`<br>2. First `# Heading` in body<br>3. `id` value converted to Title Case | Human-readable title displayed in check run summaries and reports. |
| **`paths`** | `string[]` | No | 1. Frontmatter `paths:`<br>2. `["**/*"]` (all files) | Path globs used for deterministic file filtering. |
| **`inspect`** | `string[]` | No | 1. Frontmatter `inspect:`<br>2. `["diff", "pr_body"]` | Context elements supplied to the Deep Auditor. |

### 4.2 Deterministic Derivation Rules

When optional metadata fields are omitted, implementations MUST resolve them according to the following hierarchy:

1. **`id` Derivation:**
   * If `id` is specified in frontmatter, normalize to lower kebab-case.
   * If omitted, derive from the file's path relative to its enclosing `.canons/` directory, omitting the `.md` extension.
   * *Example:* `.canons/prs-must-include-tests.md` $\rightarrow$ `id: prs-must-include-tests`.
   * *Example:* `.canons/auth/tokens-must-expire.md` $\rightarrow$ `id: auth-tokens-must-expire`.

2. **`title` Derivation:**
   * If `title` is specified in frontmatter, use verbatim.
   * Otherwise, extract the first Markdown heading (`# Heading` or `## Heading`) found in the body.
   * If no heading exists, convert the derived `id` into Title Case:
     * Strips leading numeric prefixes if present (e.g. `canon-0001-prs-must-include-tests` $\rightarrow$ `"Prs Must Include Tests"`).

3. **`paths` Derivation:**
   * If omitted, defaults to matching all repository files: `["**/*"]`.
   * For scoped canons in `<scope>/.canons/`, patterns MUST be automatically scoped to `<scope>/**`.

4. **`inspect` Derivation:**
   * If omitted, defaults to `["diff", "pr_body"]`.
   * Supported tokens include:
     * `diff`: Unified git diff of changed files.
     * `pr_body`: Pull request description / body markdown.
     * `commit_messages`: Commit messages associated with the pull request.

---

## 5. Directives & Reserved Keywords

Canon text may include reserved semantic directives to govern auditor behavior, convey author intent, and distinguish PR contributor/author obligations from automated clerk synthesis:

| Directive | Target Actor | CI Role / Verdict | Normative Contract |
| :--- | :--- | :--- | :--- |
| **None** (Default) | Contributor | **`fail`** (Blocking) | PR violates the invariant. Auditor reports the violation and failure rationale with no further advice. |
| **`Guidance`** | Contributor | **`fail`** (Blocking) | **Contributor action required.** Auditor instructs the contributor on actions needed to unblock the PR (e.g., pointing to required templates or documentation). |
| **`Supplement`** | Clerk AI | **`warn`** (Non-blocking)* | **Automated synthesis.** Auditor fulfills the invariant by synthesizing the missing material directly into the review report. |
| **`Rationale`** | Evaluator (Clerk AI) & Explainer (AI Assistant) | Informative (Explanatory context) | **Chesterton's Fence.** Explains *why* the canon exists if not self-evident. Evaluator AI uses it to disambiguate edge cases against author intent; auditor quotes or synthesizes it in check runs and reports to explain why the invariant is in place. |

> **\*Graceful Fallback Requirement:**  
> If a canon specifies a `Supplement` directive, but the Deep Auditor cannot reliably infer or synthesize the material (e.g., excessive diff complexity or ambiguous context), the implementation **MUST gracefully fall back to a blocking `fail`**, stating that automated synthesis was infeasible and that manual author action is required.

> **Rationale Normative Constraints:**
> To preserve clarity and prevent prompt dilution:
> * **Conciseness Limit:** A `Rationale` SHOULD be a single sentence (or <= 30 words). Lengthy essays MUST be deferred to external documentation files or pull request descriptions.
> * **Non-Duplication:** A `Rationale` MUST NOT merely restate the negative invariant rule (e.g., "Files must not be empty because empty files are disallowed"). It MUST articulate the underlying engineering rationale, architectural trade-off, or failure mode being prevented (Chesterton's Fence).

### 5.1 Syntax Forms

Directives MAY be expressed via inline keywords or explicit Markdown section headers:

#### A. Inline Annotation Syntax
Suitable for Tiers 1–3 canons:
```markdown
PRs introducing user-facing features must update documentation. **Guidance:** Suggest which section under `docs/` should be updated.
```
```markdown
PRs modifying UI components must include manual test scripts. **Supplement:** If feasible, synthesize a 3-step manual test script from the diff.
```
```markdown
Each canon MUST only address a single, cohesive concept for its invariant. Rationale: Multi-rule canon files increase the likelihood of flakiness, since subsequent AI evaluations may focus on different parts of the rule set.
```
```markdown
Each canon file name MUST state a testable invariant rather than a passive topic. **Rationale:** Canon Clerk derives check run titles from file stems; invariant names ensure CI reports immediately communicate expectations.
```

#### B. Section Header Syntax
Suitable for Tier 4 structured canons:
```markdown
## Guidance
Direct the author to `docs/contributing.md#test-plans` and enumerate the missing verification criteria.
```
```markdown
## Supplement
Synthesize a 3-step manual Test Script covering each modified visual state.
```
```markdown
## Rationale
Manual verification instructions ensure reviewers can reproduce visual flow and interactive state transitions that automated unit tests may miss.
```

### 5.2 Auditor Interpretation & Prompt Contract

Unlike static linters, canon evaluators do not employ a deterministic pre-parser or regex tokenizer to extract directive text blocks. Instead, the canon's raw Markdown body is passed directly into the evaluator's prompt context:

1. **Capitalized Proper Noun Signaling:** Authors SHOULD capitalize `Guidance`, `Supplement`, and `Rationale` (e.g., `**Guidance:**`, `## Guidance`, `Supplement:`, `Rationale:`, `**Rationale:**`, `## Rationale`) to clearly signal intentional directive semantics to the frontier model.
2. **Prompt-Level Behavioral Contract:** Deep Auditor system instructions MUST define the operational meaning of `Guidance`, `Supplement`, and `Rationale`, instructing the reasoning model to map them directly to its structured output payload and evaluation process:
   * **`Guidance`** $\rightarrow$ Formulate actionable author instructions in the response `guidance` field, resulting in a blocking `fail` (or `action_required`).
   * **`Supplement`** $\rightarrow$ Synthesize the requested material into the response `supplement` field, resulting in a non-blocking `warn`.
   * **`Rationale`** $\rightarrow$ Ground evaluation in author intent, and formulate explanatory context in the response `rationale` field (or synthesized review feedback) explaining why the invariant exists.
3. **Exegesis & Edge-Case Disambiguation:** The evaluator AI and downstream AI assistants MUST use `Rationale` as an interpretive lens during semantic exegesis:
   * **Edge-case disambiguation:** When diffs present borderline, ambiguous, or technically complex compliance scenarios, the evaluator AI disambiguates author intent against the stated `Rationale` rather than applying naive or superficial literalism.
   * **Explanatory check runs:** In check run reports and review comments, the auditor quotes or synthesizes the `Rationale` to explain to the PR author *why* the invariant is in place, grounding any violation or advisory in architectural context.
4. **Resilience to Variation:** Because evaluation is performed contextually by the reasoning model rather than through rigid AST pattern-matching, minor natural language phrasing variations (e.g., `**Guidance for author:**`, `### Guidance`, `Rationale:`, `**Rationale:**`, or `### Why this rule exists`) remain fully functional.

---

## 6. Progressive Disclosure Tiers *(Informative)*

Canons scale smoothly across four progressive tiers to balance simplicity with fine-grained control:

### Tier 1: Minimal Viable Canon (Zero Friction)
A plain Markdown assertion with no frontmatter, headers, or metadata boilerplate:
```markdown
PRs that introduce new user-facing features must have accompanying documentation in `docs/`.
```

### Tier 2: Keyword Directives
A plain Markdown assertion augmented with inline `**Guidance:**`, `**Supplement:**`, or `**Rationale:**` / `Rationale:` directives:
```markdown
PRs modifying user-facing UI components MUST include a manual Test Script in the PR description. **Supplement:** If feasible, synthesize a candidate manual Test Script from the PR diff and description.
```

### Tier 3: Cost-Optimized Canon
A canon adding YAML frontmatter (`paths:`) purely to enable deterministic Stage 0 path filtering at zero token cost:
```markdown
---
paths:
  - "src/components/**"
  - "public/**"
---
PRs modifying user-facing UI components MUST include a manual Test Script in the PR description. **Supplement:** If feasible, synthesize a candidate manual Test Script from the PR diff and description.
```

### Tier 4: Structured / Multi-Section Canon
A fully structured canon containing explicit sections (`## Rule`, `## Rationale`, `## Evaluation Criteria`, `## Guidance`, `## Supplement`) for complex policies requiring detailed rubrics:
```markdown
---
title: Manual Test Script Required for UI Changes
paths:
  - "src/ui/**"
  - "frontend/**"
inspect:
  - pr_body
  - diff
---

## Rule
Any pull request modifying user-facing UI components must include a numbered `### Manual Test Script` in the PR description.

## Rationale
Manual verification instructions ensure reviewers can reproduce visual flow and interactive state transitions that automated unit tests may miss.

## Evaluation Criteria
- **Inapplicable**: Pure refactors, internal types, or non-visual changes with zero visual impact.
- **Pass**: Description contains clear, reproducible manual verification steps.
- **Fail**: UI components changed, but no manual test steps are present, or steps are ambiguous.

## Supplement
Analyze the modified UI components and synthesize a candidate manual test script conforming to industry standards.
```

---

## 7. File Naming & Numbering Conventions

1. **Assertive Invariant Slugs (RECOMMENDED):**  
   Canons SHOULD be named using descriptive kebab-case slugs that state the invariant being enforced (e.g., `prs-must-document-new-features.md`, `services-must-maintain-bounded-contexts.md`).
2. **Sequential Numbering (OPTIONAL):**  
   Teams that prefer numeric prefixing (e.g., `canon-0001-prs-must-include-tests.md`) MAY use it as an organizational convention. Sequential numbering is strictly non-normative and MUST NOT be required by the engine.
3. **Collision Resistance:**  
   Because each canon maps to a distinct file path, collisions are structurally impossible. Implementations SHOULD display each canon's repository-relative path in user interfaces for disambiguation.

---

## 8. Execution & Architecture *(Informative)*

This specification defines the grammar, document model, and derivation rules for authoring canons. For the architecture of the three-stage evaluation cascade (Deterministic Path Filter $\rightarrow$ Screener LLM $\rightarrow$ Deep Auditor LLM), context extraction, and GitHub Check Run verdict mapping, see **[Architecture & Evaluation Cascade](docs/architecture.md)**.
