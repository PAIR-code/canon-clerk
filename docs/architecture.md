# Architecture & Evaluation Cascade

**Status:** Aspirational Architectural Waypoint (Formal machine contracts will be codified in OpenSpec).

---

## 1. Overview & Motivation

Canon Clerk is designed to audit pull requests against natural language project invariants without incurring the latency and cost of running large reasoning models over every canon for every commit.

To balance token economics, latency, and audit rigor, Canon Clerk executes audits through a **three-stage evaluation cascade**:

```mermaid
flowchart LR
    PR[Pull Request] --> S0["Stage 0: Path Filter<br/>(Deterministic Globs)"]
    S0 -- "Matched Canons<br/>(0 tokens)" --> S1["Stage 1: Screener<br/>(Fast LLM)"]
    S1 -- "Applicable Canons" --> S2["Stage 2: Deep Auditor<br/>(Reasoning LLM)"]
    S2 --> Verdict[PR Verdict & Annotations]
```

By cascading from deterministic filters to lightweight screening and finally to deep reasoning, Canon Clerk minimizes API costs while maintaining high-fidelity review gates.

---

## 2. The Three-Stage Cascade

### Stage 0: Path Filter (Deterministic)
* **Goal:** Instantly discard canons whose file boundaries do not intersect with the changes in the pull request.
* **Cost & Latency:** 0 tokens, near-instantaneous execution.
* **Mechanism:**
  * Compares the list of modified files against each canon's `paths:` globs.
  * Honors monorepo package boundaries: a canon residing in `<scope>/.canons/` automatically inherits an implicit `<scope>/**` path filter.
  * If a canon specifies no `paths:` filter and is located at root, it defaults to `["**/*"]` and always passes Stage 0.

### Stage 1: Screener (Fast LLM or System One Model)
* **Goal:** Rapidly determine which remaining candidate canons are plausibly applicable based on high-level PR context.
* **Model Class:** Fast, low-latency models (e.g., Gemini Flash).
* **Inputs:**
  * Pull request metadata: title, description/body, branch name.
  * Diff statistics: touched file list, change counts (lines added/removed).
  * Canon summaries: title, invariant statement, evaluation criteria.
* **Outputs:** A filtered list of canons flagged as potentially applicable.
* **CI Lifecycle:** Canons passing Stage 1 transition into in-progress GitHub Check Runs.

### Stage 2: Deep Auditor (Reasoning LLM)
* **Goal:** Deeply analyze the actual changes to reach a definitive compliance verdict, generate helpful guidance, or synthesize missing artifacts.
* **Model Class:** Frontier reasoning models (e.g., Gemini Pro with reasoning/thinking enabled).
* **Inputs:**
  * Unified git diff of modified files.
  * Scoped context specified by the canon's `inspect:` frontmatter (`diff`, `pr_body`, `commit_messages`).
  * Full canon text (rule, evaluation criteria, guidance/supplement directives).
* **Outputs:** Structured verdict, failure rationale, line-level code annotations, and synthesized material (if requested).

---

## 3. Directives & Execution Mechanics

When evaluating canons that fail compliance, the Deep Auditor looks for two first-class directives defined in [SPEC.md](../SPEC.md):

### 3.1 `Guidance` (Contributor Directive)
* **Actor:** Contributor (Human or AI agent).
* **CI Verdict:** **`fail`** (or **`action_required`** for PR metadata/process issues). Both are **blocking**.
* **Behavior:** The auditor details what the PR author must do to bring the change into compliance (e.g., pointing to required templates, documentation sections, or missing test scenarios).

### 3.2 `Supplement` (Clerk Synthesis)
* **Actor:** Clerk AI.
* **CI Verdict:** **`warn`** (Non-blocking / GitHub conclusion: `neutral`).
* **Behavior:** The Clerk cures the defect on the fly by synthesizing the missing asset (e.g., drafting a 3-step manual test script from the diff) and appending it directly to the Check Run summary.
* **Graceful Fallback:** If the diff is too ambiguous or complex for the auditor to feasibly synthesize the material, the Clerk **gracefully falls back to a blocking `fail`**, explaining that automated synthesis was infeasible and that manual author action is required.

---

## 4. Verdict Matrix & CI Integration

Each canon evaluated by the Deep Auditor completes its GitHub Check Run with one of five conclusions:

| Engine Verdict | GitHub Check Run Conclusion | Blocks Merge? | Scope | Description |
| :--- | :--- | :--- | :--- | :--- |
| **`pass`** | `success` 🟢 | No | Both | Canon applies and the PR fully conforms to the invariant. |
| **`fail`** | `failure` 🔴 | **Yes** | **Source Code** | Canon applies, but code violates the invariant. Includes cases where `Guidance` was provided or a `Supplement` attempt was infeasible. |
| **`action_required`** | `action_required` 🟡 | **Yes** | **Metadata & Process** | Canon applies, but PR metadata/process violates the invariant (e.g., missing test plan, invalid PR title). |
| **`warn`** | `neutral` ⚪ | No | Both | Non-blocking advisory, or applied when the Clerk successfully synthesizes a `Supplement` (curing the defect). |
| **`skipped`** | `skipped` ⚪ | No | N/A | Canon determined not to interact with this PR upon deep inspection. *(Safety valve for optimistic Stage 1 screening).* |

---

## 5. Future Evolution: Spec-Driven Development

As outlined in [Issue #8](https://github.com/PAIR-code/canon-clerk/issues/8), these architectural contracts will graduate into formal, machine-verifiable specifications using OpenSpec:
* `specs/core/cascade-contracts.md`: Formal schemas and caching boundaries for Stages 0, 1, and 2.
* `specs/cli/interface.md`: CLI commands, flags, stdin/stdout protocols, and exit codes.
* `specs/action/interface.md`: GitHub Action inputs, outputs, and Check Run API contracts.
