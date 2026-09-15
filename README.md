# Canon Clerk

**Problem:** AI tools have accelerated and automated code generation, making project maintainers and custodians acute bottlenecks. Maintainers bear an asymmetric cognitive tax, reverse-engineering unsolicited but plausible AI-assisted PRs that pass existing tests but quietly violate unwritten architectural rules and project tenets.

**Solution:** Canon Clerk prescribes a natural language Markdown+YAML schema for _project canons_ which live in `.canons/` directories. Through its CLI or GitHub Action, Canon Clerk checks proposed changes against canons for applicability and conformance. By gating CI on canon adherence, Canon Clerk preserves maintainer attention for truly novel situations.

---

## Overview

Traditional AST-based linters excel at syntax and static analysis, but fail on semantic guidelines that require contextual comprehension:
- *"Did this UI change include a reproducible manual test plan in the description?"*
- *"Does this new service violate our bounded context isolation boundaries?"*
- *"Are customer-facing API error messages conforming to our voice-and-tone standards?"*

---

## How It Works

Canon Clerk runs locally or in CI through an efficient three-stage cascade:

```mermaid
flowchart LR
    PR[Pull Request] --> S0[Stage 0: Path Filter\n(Deterministic Globs)]
    S0 -- "0 tokens" --> S1[Stage 1: Screener\n(Fast LLM)]
    S1 -- "Filtered Canons" --> S2[Stage 2: Deep Auditor\n(Reasoning LLM)]
    S2 --> Verdict[PR Verdict & Annotations]
```

1. **Stage 0: Path Filter (Deterministic)**  
   Instantly discards canons whose location or file globs don't match the PR's modified files (zero cost, zero latency).
2. **Stage 1: Screener (Fast LLM)**  
   Batches remaining candidate canons using only PR metadata and diff statistics to filter for possible applicability. Possibly applicable canons become POST'ed Check Runs in progress.
3. **Stage 2: Deep Auditor (Reasoning LLM)**  
   Audits the actual git diff and context against only the screened-in canons, returning a structured verdict (`pass`, `fail`, `action_required`, `advisory`, or `skipped`). Finished canon analyses PATCH Check Runs with results.

---

## Defining a Canon

Store canons as Markdown files with YAML frontmatter in `.canons/` at the project root or within scoped subdirectories.

Example:

```markdown
---
id: canon-0001-ui-manual-test-plan
title: Manual Test Plan Required for UI Changes
paths:
  - "src/ui/**"
  - "frontend/**"
inspect:
  - pr_body
  - diff
---

## Rule
Any pull request modifying user-facing UI components must include a numbered `### Manual Test Plan` in the PR description.

## Evaluation Criteria
- **Inapplicable**: Pure refactors or internal types with zero visual/behavioral impact.
- **Pass**: Description contains reproducible manual verification steps.
- **Fail**: UI components changed, but no manual test steps are present, or are poorly worded or ambiguous.

## Remediation (Optional)
When failing, analyze the modified UI components and synthesize a candidate manual test plan if feasible. Append the draft to the Check Run summary.

Do not generate a candidate manual test plan if the changes are purely non-visual styling tokens or build assets.
```

> **Note on Remediation**: The `## Remediation` section is entirely optional. When present, the Deep Auditor uses its instructions and templates to offer actionable suggestions in the Check Run report. When omitted, the auditor strictly reports the verdict and failure rationale without offering unsolicited guidance.

### Frontmatter Fields

Frontmatter fields serve as deterministic routing and cost-optimization hints:

| Field | Required? | Default | Description |
| :--- | :--- | :--- | :--- |
| **`id`** | **Yes** | — | Unique machine identifier used in Check Runs and file naming. |
| **`title`** | **Yes** | — | Human-readable title displayed in GitHub Check Run headers. |
| **`paths`** | No | `["**/*"]` | **Stage 0 Optimization.** File globs used to filter applicability at zero token cost. If omitted, the canon passes to Stage 1 for all PRs. |
| **`inspect`** | No | `["diff", "pr_body"]` | **Stage 2 Optimization.** Limits context sent to the Deep Auditor (`diff`, `pr_body`, `commit_messages`). |

> **Design Philosophy**: *Canon Clerk is designed to enforce your project's opinions, not impose its own.* Optional fields degrade gracefully, potentially increasing token usage rather than failing with rigid schema errors.

### Naming & ID Conventions (Recommended)

While Canon Clerk treats the relative file path as the authoritative unique identifier, we recommend the following conventions:
- **File Name & ID Symmetry:** Name files `<id>.md`, matching the `id` declared in the frontmatter.
- **Kebab-Case with Slug:** Use `canon-<number>-<slug>` (e.g., `canon-0001-ui-manual-test-plan.md`).

Including a descriptive slug disambiguates rules in GitHub Check Run titles and avoids ID collisions when scoped `.canons/` directories exist across monorepo subprojects.

---

## Quickstart (GitHub Actions)

Add the following workflow to `.github/workflows/canon-clerk.yml`:

```yaml
name: Canon Clerk

on:
  pull_request:
    types: [opened, synchronize, reopened, edited]

jobs:
  audit:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      pull-requests: write
    steps:
      - name: Checkout Code
        uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Run Canon Clerk
        uses: pair-code/canon-clerk@v1
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          screener_model: gemini-3.7-flash
          auditor_model: gemini-3.5-pro
        env:
          GEMINI_API_KEY: ${{ secrets.GEMINI_API_KEY }}
```

---

## Verdicts & Feedback

Each canon evaluated by the Deep Auditor completes its GitHub Check Run with one of five conclusions:

| Verdict | GitHub Conclusion | Blocks Merge? | Scope | Description |
| :--- | :--- | :--- | :--- | :--- |
| **`pass`** | `success` 🟢 | No | Both | Canon applies and the PR is compliant. |
| **`fail`** | `failure` 🔴 | **Yes** | **Source Code** | Canon applies, but code is non-compliant (e.g. boundary violations, forbidden dependencies). Check Run body *may* contain Remediation guidance. |
| **`action_required`** | `action_required` 🟡 | **Yes** | **Metadata & Process** | Canon applies, but PR metadata/process is non-compliant (e.g. missing manual test plan, required tags, commit format). Check Run body *should* contain Remediation guidance. |
| **`advisory`** | `neutral` ⚪ | No | Both | Canon applies and PR is marginally compliant. Non-blocking; Check Run body *should* contain Remediation guidance. |
| **`skipped`** | `skipped` ⚪ | No | N/A | Canon determined not to interact with this PR upon deep inspection. *(Expected to be rare; acts as a safety valve when optimistic Stage 1 screening flags a canon that proves inapplicable on diff inspection).* |

---

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](docs/contributing.md) for details on how to get started.

## License

Apache 2.0 - See [LICENSE](LICENSE) for details.

---

> *Disclaimer: This is not an officially supported Google product.*
