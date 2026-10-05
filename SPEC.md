# Canon Format Specification

**Document Status:** Living Standard  
**Authority:** Normative definition governing canon parsers, linters, and evaluation engines.

---

## 1. Scope & Conformance

### 1.1 Purpose
This specification defines the syntax, schema, progressive disclosure tiers, and metadata derivation rules for **canons**—declarative, source code or repository change rules (both portable standard rule packs and repository-specific extensions) stored in `.canons/` directories that are semantic, atomic, falsifiable, bounded, grounded, salient, and correctable.

### 1.2 Conformance Terminology
The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in BCP 14 [[RFC 2119](https://www.rfc-editor.org/rfc/rfc2119)] [[RFC 8174](https://www.rfc-editor.org/rfc/rfc8174)] when, and only when, they appear in all capitals, as shown here.

### 1.3 Core Principles
1. **Zero Barrier to Entry:** A single sentence in a plain text Markdown file MUST be treated as a fully valid, enforceable canon.
2. **Progressive Disclosure:** Advanced optimizations (deterministic path filtering via `triggers:`, custom inspection scopes, and structured rubrics) are strictly OPTIONAL.
3. **The What / When / Why / How Tetrad:** Canons scale along an intuitive cognitive architecture:
   * **What (The Invariant):** The boundary condition that MUST be true (formulated using RFC 2119 keywords).
   * **When (Permissible Deviations — `Exception`):** Strict, verifiable conditions under which an invariant violation is waived.
   * **Why (Precedent Reasoning — `Rationale`):** The underlying engineering rationale or Chesterton's Fence explaining why the invariant exists.
   * **How (Remediation):** Actionable contributor remediation instructions detailing how to achieve compliance upon failure.
   While Tier 1 canons require only the "What", mature canons naturally synthesize this tetrad.
4. **Clarity of Action:** The specification strictly defines an enforcement boundary: Canon Clerk evaluates invariants and emits actionable `Remediation` instructions, leaving artifact synthesis to the contributor or their client-side coding agent.

### 1.4 The Seven Canon Attributes
> **Canon:** *(n)*. A source code or repository change rule that is: **semantic**, **atomic**, **falsifiable**, **bounded**, **grounded**, **salient**, and **correctable**.

A conforming canon adheres to the following heptad of core attributes:
1. **Semantic:** A canon MUST govern architectural intent, conventions, or domain policies that cannot be evaluated purely deterministically by an AST parser or regular expression. Constraints verifiable via static analysis are forbidden.
2. **Atomic:** A canon MUST address a single cohesive invariant to prevent compound evaluation flakiness.
3. **Falsifiable:** A canon MUST define a testable, binary condition of violation; unfalsifiable or purely aspirational assertions are forbidden.
4. **Bounded:** A canon MUST demarcate its applicability via explicit directory scoping, trigger path patterns, or verified exception conditions.
5. **Grounded:** A canon MUST be warranted by authoritative precedent, industry standards, or documented architectural trade-offs (Chesterton's Fence) rather than arbitrary personal preference.
6. **Salient:** A canon MUST maximize signal density and employ precise domain terminology tailored for an AI readership, omitting needless words and conversational padding.
7. **Correctable:** A canon MUST admit a viable path to compliance, whether through explicit remediation instructions or implicit action.

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
| **`triggers`** | `string[]` | No | 1. Frontmatter `triggers:`<br>2. `["**/*"]` (all files) | Path globs activating the canon for rule evaluation. |
| **`inspect`** | `string[]` | No | 1. Frontmatter `inspect:`<br>2. `["diff", "pr_title?", "pr_body?"]` | Context elements inspected during evaluation (supports `?` optional rider). |
| **`tags`** | `string[]` | No | 1. Frontmatter `tags:`<br>2. `[]` (empty list) | Categorical labels used for topical organization, cataloging, and selective filtering. |
| **`references`** | `string[]` | No | 1. Frontmatter `references:`<br>2. `[]` (empty list) | Persistent repository reference files supplied as grounding context. |

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

3. **`triggers` Derivation (Deterministic Path Filtering):**
   * If `triggers` is specified in frontmatter, resolve globs as path patterns that activate the canon when matched by any modified file in the pull request.
   * If omitted, defaults to matching all repository files: `["**/*"]`.
   * **Scoped Canons:** For scoped canons in `<scope>/.canons/`, patterns MUST be automatically scoped to `<scope>/**`.

4. **`inspect` Derivation:**
   * If omitted, defaults to `["diff", "pr_title?", "pr_body?"]`.
   * Supported base tokens include:
     * `diff`: Unified git diff of changed files.
     * `pr_title`: Pull request title text.
     * `pr_body`: Pull request description / body markdown.
     * `commit_messages`: Commit messages associated with the pull request.
     * `linked_issues`: Titles, bodies, and metadata of issues linked to or referenced by the pull request.
   * **Required vs. Optional Modifiers (`?` Rider):**
     * **Required Plane (Default, e.g. `diff`, `pr_body`):** The filing MUST contain this exhibit plane for the canon to apply. A canon will be pruned during `discover` unless all of its declared required exhibit planes are satisfied in the intake filing.
     * **Optional Plane (`?` Suffix, e.g. `pr_title?`, `pr_body?`):** An aspirational exhibit plane. If present in the intake filing, it is retained and admitted as evidence. If absent, the canon is NOT pruned and evaluates without that exhibit.
     * **All-Optional Edge Case:** If all declared `inspect` tokens bear the `?` modifier (e.g. `[diff?, pr_body?]`), the canon requires at least one of the declared planes to be present in the intake filing.
   * **Runtime Resolution Semantics for `pr_title`:**
     * In GitHub Actions and CI webhook environments, `pr_title` evaluates to the active pull request title.
     * In local CLI environments (e.g. evaluating uncommitted changes or a local branch prior to opening a PR), `pr_title` gracefully evaluates to the commit subject of the current `HEAD` commit (or an empty string if no commits exist).
   * **Discovery & Boundary Semantics for `linked_issues`:**
     * **Discovery:** Implementations MUST discover linked issues via:
       1. Explicit pull request closing references via GitHub's API (`closingIssuesReferences`).
       2. Standard closing keywords in the pull request body matching the case-insensitive pattern `(close[sd]?|fix(e[sd])?|resolve[sd]?)\s+#(\d+)`.
     * **Payload Boundary:** For each discovered issue, the injected payload MUST include the issue number, title, author, labels, and issue body Markdown.
     * **Token Preservation Boundary:** To preserve context window hygiene, issue comments and reaction trails MUST be excluded by default.

5. **`tags` Derivation:**
   * If omitted, defaults to an empty list: `[]`.
   * **Scalar Coercion:** If specified in frontmatter as a single scalar string (e.g. `tags: canon-authoring`), implementations MUST coerce it to a single-element list (`["canon-authoring"]`).
   * **Normalization:** Each tag MUST be normalized to lower kebab-case:
     * Convert characters to lowercase.
     * Replace whitespace and underscore characters (`_`) with hyphens (`-`).
     * Strip invalid characters (retaining only lowercase letters, digits, and hyphens).
     * Collapse consecutive hyphens into a single hyphen, and trim leading and trailing hyphens.
     * Empty or whitespace-only tags MUST be discarded.
     * *Example:* `tags: [Architecture, "API Design", core_module]` $\rightarrow$ `["architecture", "api-design", "core-module"]`.
   * **Deduplication:** Implementations MUST deduplicate tags while preserving declaration order.

6. **`references` Derivation:**
   * If omitted, defaults to an empty list: `[]`.
   * **Scalar Coercion:** If specified in frontmatter as a single scalar string (e.g. `references: "docs/architecture.md"`), implementations MUST coerce it to a single-element list (`["docs/architecture.md"]`).
   * **Resolution Target:** Path globs in `references` MUST be resolved against the repository checkout at the pull request's `HEAD` commit.
   * **Scoped Canons:** For scoped canons in `<scope>/.canons/`, relative glob patterns MUST be automatically scoped to `<scope>/**` unless explicitly anchored with a leading `/` (e.g. `/docs/**` or `/package.json`).
   * **Grounding Context Injection:** Resolved reference files MUST be read from disk and provided to the evaluator prompt within dedicated semantic context blocks (e.g. `<reference_documents>`), separate from the active pull request `<diff>`.
   * **Safety Caps & Token Hygiene:**
     * **Text Files Only:** Non-text or binary files (e.g. images, compiled artifacts, archives) MUST be excluded.
     * **File Count Limit:** Implementations MUST NOT load more than **5 files** per canon by default.
     * **Byte Size Limit:** Total reference content per canon MUST NOT exceed **32KB** by default; content exceeding this cap MUST be truncated with a visible diagnostic notice.

7. **`invariant` Derivation (The Invariant Statement):**
   * If a Markdown body contains text outside recognized directive blocks (`Exception`, `Rationale`, `Remediation`), the first text block (excluding any opening `#` or `##` heading) MUST be used as the invariant statement.
   * If the Markdown body is empty, or consists solely of headings or directives with zero invariant body text (e.g., a 0-byte file such as `all-caps-spec-must-refer-to-spec-md.md` or a heading-only file `# PRs Must Include Tests`), implementations MUST fall back to using the derived `title` as the invariant statement.
   * *Rationale:* Adhering to Principle 1.3.1 (Zero Barrier to Entry) and canon naming standards (`canon-names-must-state-invariants`), a canon's filename or heading embodies its normative policy. This guarantees that minimal, 0-byte, or heading-only files function as valid, evaluable canons without producing empty evaluation prompts.

---

## 5. Directives & Reserved Keywords

Canons scale across an intuitive **What / When / Why / How** cognitive tetrad:
* **What (The Invariant):** The normative boundary condition specifying what MUST or MUST NOT be true (formulated with RFC 2119 keywords).
* **When (Permissible Deviations):** The `Exception` directive defining strict conditions under which an invariant violation is waived.
* **Why (The Rationale):** The `Rationale` directive articulating *why* the invariant exists (Chesterton's Fence).
* **How (Remediation):** The `Remediation` directive instructing the contributor on actionable steps required to achieve compliance upon failure.

Canon text may include reserved semantic directives to govern auditor behavior, convey author intent, and direct contributor remediation:

| Directive | Tetrad Role | Target Actor | CI Role / Verdict | Normative Contract |
| :--- | :--- | :--- | :--- | :--- |
| **None** (Default) | **What** | Contributor | **`fail`** (Blocking) | PR violates the invariant. Auditor reports the violation and failure rationale with no further advice. |
| **`Exception`** | **When** (Permissible Deviations) | Evaluator (Clerk AI) | Conditional **`pass`** | Defines strict conditions under which an invariant violation is waived. If conditions are met semantically, verdict transitions from `fail` $\rightarrow$ `pass`. |
| **`Rationale`** | **Why** | Evaluator (Clerk AI) & Explainer (AI Assistant) | Informative (Explanatory context) | **Chesterton's Fence.** Explains *why* the canon exists if not self-evident. Evaluator AI uses it to disambiguate edge cases against author intent; auditor quotes or synthesizes it in check runs and reports to explain why the invariant is in place. |
| **`Remediation`** | **How** | Contributor | **`fail`** (Blocking) | **Contributor action required.** Auditor instructs the contributor on actions needed to unblock the PR (e.g., pointing to required templates or documentation). |

> **Exception Normative Constraints & Auditor Behavioral Contracts:**
> Real-world engineering invariants frequently admit legitimate escape hatches (e.g. performance hot-paths, transitional shims, or vendor workarounds). To ensure escape hatches preserve architectural integrity:
> * **Short-Circuit to Pass:** When an invariant violation is detected, the evaluator AI screens the diff and pull request context against declared `Exception` clauses. If all criteria of an exception are met semantically, the check short-circuits and resolves to `pass`, recording the matched exception clause and justification in the Check Run summary.
> * **Semantic Sufficiency Over Syntactic Presence:** Unlike deterministic linters that only verify comment flags (such as `// eslint-disable-next-line` or `// canon-ignore`), canon evaluators MUST evaluate the *substantive content*, factual grounding, and truthfulness of the justification against the diff and PR context. A hand-wavy or tautological comment (e.g., `// bespoke: this is faster`) MUST fail an exception demanding specific, measurable performance benchmarks or CVE isolation.
> * **Plurality & Independent Evaluation (Logical OR):** Multiple discrete `Exception` clauses evaluate independently as logical ORs. If any single exception clause is fully satisfied, the invariant violation is waived.
> * **Biconditional Formulation (ADVISORY):** Authors SHOULD formulate `Exception` clauses using RFC 2119 keywords specifying permissible deviation (e.g. `MAY be introduced IFF accompanied by...`). The keyword `IFF` (if and only if) establishes a rigorous, testable biconditional boundary.
> * **Orthogonality & Antecedent Gating:** `Exception` is orthogonal to `Rationale` and `Remediation`. It acts as an antecedent gate before contributor remediation: if an exception is met, the check passes without triggering `Remediation`. If all exception clauses fail, the violation stands, executing `Remediation` (`fail`) or default blocking `fail`.

> **Rationale Normative Constraints & Advisory Guidance:**
> To preserve clarity and prevent prompt dilution:
> * **Conciseness Limit:** A `Rationale` SHOULD be a single sentence (or <= 30 words). Lengthy essays MUST be deferred to external documentation files or pull request descriptions.
> * **Non-Duplication:** A `Rationale` MUST NOT merely restate the negative invariant rule (e.g., "Files must not be empty because empty files are disallowed"). It MUST articulate the underlying engineering rationale, architectural trade-off, or failure mode being prevented (Chesterton's Fence).
> * **Citing Governing Standards & Precedents (ADVISORY):** When a canon invariant codifies an established domain practice, formal specification, or industry consensus, authors SHOULD explicitly cite the governing standard or precedent (e.g. RFC 2119, POSIX.1-2017, W3C WCAG, IEEE 754, SemVer 2.0.0, The Twelve-Factor App, or clig.dev) directly within the invariant or `Rationale:` directive.  
>   *AI Evaluator Latent Anchoring:* Explicit standard citations act as high-affinity latent anchors for frontier reasoning models, activating pre-trained clusters of architectural intent, edge-case nuances, and industry consensus without verbose prompt overhead.

### 5.1 Syntax Forms

Directives MAY be expressed via inline keywords or explicit Markdown section headers:

#### A. Inline Annotation Syntax
Suitable for Tiers 1–3 canons:
```markdown
Standard platform APIs MUST be used rather than introducing bespoke implementations.
Exception: A bespoke implementation MAY be introduced IFF accompanied by an inline comment explaining a specific, measurable performance result.
Exception: A bespoke implementation MAY be introduced IFF isolating a documented security vulnerability or CVE.
Rationale: Custom utility functions increase maintenance drag and duplicate vetted runtime capabilities.
**Remediation:** Replace custom utilities with platform equivalents, or supply the required benchmark/CVE documentation.
```
```markdown
PRs introducing user-facing features must update documentation. **Remediation:** Suggest which section under `docs/` should be updated.
```
```markdown
PRs modifying UI components must include manual test scripts. **Remediation:** Document a 3-step manual test script in the PR description verifying user interaction.
```
```markdown
Each canon MUST only address a single, cohesive concept for its invariant. Rationale: Multi-rule canon files increase the likelihood of flakiness, since subsequent AI evaluations may focus on different parts of the rule set.
```
```markdown
Each canon file name MUST state a testable invariant rather than a passive topic. Rationale: Canon Clerk derives check run titles from file stems; invariant names ensure CI reports immediately communicate expectations.
```

> **Typographic Styling & Visual Affordances (Informative):**  
> Directives in raw Markdown are recognized with or without bold asterisks (e.g. `Exception:`, `**Exception:**`, `Rationale:`, `**Rationale:**`, `Remediation:`, `**Remediation:**`). In reference exemplars, bolding `**Remediation:**` while leaving `Exception:` and `Rationale:` unbolded is recommended as an ergonomic convention: `**Remediation:**` acts as a high-contrast visual call-to-action for human and agent contributors remediating failures, while `Exception:` and `Rationale:` serve as unbolded explanatory context for the evaluator model.

#### B. Section Header Syntax
Suitable for Tier 4 structured canons:
```markdown
## Rule
Standard, built-in platform functions MUST be used rather than introducing bespoke implementations.

## Exceptions
- A bespoke implementation MAY be introduced IFF accompanied by an inline comment explaining a specific, measurable performance result.
- A bespoke implementation MAY be introduced IFF isolating a documented security vulnerability or CVE.

## Rationale
Custom utility functions increase maintenance drag and duplicate vetted runtime capabilities.

## Remediation
Replace custom utilities with platform equivalents, or supply the required benchmark/CVE documentation.
```
```markdown
## Remediation
Direct the author to `docs/contributing.md#test-plans` and enumerate the missing verification criteria.
```
```markdown
## Rationale
Manual verification instructions ensure reviewers can reproduce visual flow and interactive state transitions that automated unit tests may miss.
```

> **Section Header Plurality:**  
> Implementations MUST recognize both `## Exceptions` (plural) and `## Exception` (singular) for Tier 4 section header syntax.

### 5.2 Auditor Interpretation & Prompt Contract

Unlike static linters, canon evaluators do not employ a deterministic pre-parser or regex tokenizer to extract directive text blocks. Instead, the canon's raw Markdown body is passed directly into the evaluator's prompt context:

1. **Capitalized Proper Noun Signaling:** Authors SHOULD capitalize `Exception`, `Remediation`, and `Rationale` (e.g., `Exception:`, `**Exception:**`, `## Exceptions`, `## Exception`, `**Remediation:**`, `## Remediation`, `Rationale:`, `**Rationale:**`, `## Rationale`) to clearly signal intentional directive semantics to the frontier model.
2. **Prompt-Level Behavioral Contract:** Evaluator system instructions MUST define the operational meaning of `Exception`, `Remediation`, and `Rationale`, instructing the reasoning model to map them directly to its structured output payload and evaluation process:
   * **`Exception`** $\rightarrow$ Screen diff and pull request context against declared exception criteria; if semantically satisfied, short-circuit verdict to conditional `pass`, recording the matched exception clause and justification in the audit report.
   * **`Remediation`** $\rightarrow$ Formulate actionable author instructions in the response `remediation` field, resulting in a blocking `fail` (or `action_required`). Evaluator implementations SHOULD accept legacy `Guidance` as an alias during semantic exegesis.
   * **`Rationale`** $\rightarrow$ Ground evaluation in author intent, and formulate explanatory context in the response `rationale` field (or synthesized review feedback) explaining why the invariant exists.
3. **Exegesis & Edge-Case Disambiguation:** The evaluator AI and downstream AI assistants MUST use `Rationale` as an interpretive lens during semantic exegesis:
   * **Edge-case disambiguation:** When diffs present borderline, ambiguous, or technically complex compliance scenarios, the evaluator AI disambiguates author intent against the stated `Rationale` rather than applying naive or superficial literalism.
   * **Latent standard anchoring:** Citations to authoritative standards (e.g. POSIX, RFCs, SemVer, clig.dev) ground the model in established industry definitions, eliminating hallucinations or arbitrary stylistic debates during compliance screening.
   * **Explanatory check runs:** In check run reports and review comments, the auditor quotes or synthesizes the `Rationale` to explain to the PR author *why* the invariant is in place, grounding any violation or advisory in architectural context.
4. **Resilience to Variation:** Because evaluation is performed contextually by the reasoning model rather than through rigid AST pattern-matching, minor natural language phrasing variations (e.g., `**Remediation for author:**`, `### Remediation`, `**Guidance:**`, `Rationale:`, `**Rationale:**`, or `### Why this rule exists`) remain fully functional.

---

## 6. Progressive Disclosure Tiers *(Informative)*

Canons scale smoothly across four progressive tiers to balance simplicity with fine-grained control:

### Tier 1: Minimal Viable Canon (Zero Friction)
A plain Markdown assertion with no frontmatter, headers, or metadata boilerplate:
```markdown
PRs that introduce new user-facing features must have accompanying documentation in `docs/`.
```

### Tier 2: Keyword Directives
A plain Markdown assertion augmented with inline `Exception:`, `**Remediation:**`, or `Rationale:` directives:
```markdown
PRs modifying user-facing UI components MUST include a manual Test Script in the PR description. **Remediation:** Document a 3-step manual test script in the PR description verifying user interaction.
```
```markdown
Standard platform APIs MUST be used rather than introducing bespoke implementations. Exception: A bespoke implementation MAY be introduced IFF accompanied by an inline comment explaining a specific, measurable performance result. Rationale: Bespoke utilities duplicate runtime capabilities and increase maintenance drag. **Remediation:** Replace custom utilities with platform equivalents or provide the required benchmark documentation.
```

### Tier 3: Cost-Optimized Canon
A canon adding YAML frontmatter (`triggers:`) purely to enable deterministic path filtering at zero token cost:
```markdown
---
triggers:
  - "src/components/**"
  - "public/**"
---
PRs modifying user-facing UI components MUST include a manual Test Script in the PR description. **Remediation:** Document a 3-step manual test script in the PR description verifying user interaction.
```

### Tier 4: Structured / Multi-Section Canon
A fully structured canon containing explicit sections (`## Rule`, `## Exceptions`, `## Rationale`, `## Evaluation Criteria`, `## Remediation`) for complex policies requiring detailed rubrics:
```markdown
---
title: Manual Test Script Required for UI Changes
triggers:
  - "src/ui/**"
  - "frontend/**"
tags:
  - testing
  - ui
references:
  - "docs/testing-standards.md"
inspect:
  - pr_body
  - diff
---

## Rule
Any pull request modifying user-facing UI components must include a numbered `### Manual Test Script` in the PR description.

## Exceptions
- Manual test scripts MAY be omitted IFF the pull request touches exclusively headless stylesheet refactors with zero layout or DOM alterations, accompanied by automated visual regression test output.

## Rationale
Manual verification instructions ensure reviewers can reproduce visual flow and interactive state transitions that automated unit tests may miss.

## Evaluation Criteria
- **Inapplicable**: Pure refactors, internal types, or non-visual changes with zero visual impact.
- **Pass**: Description contains clear, reproducible manual verification steps, or satisfies an Exception clause.
- **Fail**: UI components changed, but no manual test steps are present, or steps are ambiguous.

## Remediation
Add a numbered `### Manual Test Script` section to the PR description outlining step-by-step verification instructions, or supply visual regression test artifacts if eligible for an exception.
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

This specification defines the grammar, document model, and derivation rules for authoring canons. For the architecture of the Caseload DAG execution model, context extraction, and GitHub Check Run verdict mapping, see **[Architecture & The Caseload Pipeline](docs/architecture.md)** (with overview at **[Caseload DAG Overview](docs/architecture/overview.md)**).
