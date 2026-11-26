# Exhibit Materialization, Overlay & Rendering

**Status:** Authoritative Architectural Standard  
**Domain Concept:** Cross-Cutting Exhibit Rendering, Workspace Patch Overlay, 1-Based Line Coordinates, Span Tiling & Strategy D Skeletons

---

## 1. Architectural Role & Cross-Cutting Scope

Canon Clerk evaluates filings across three exhibit-consuming stages—`admit` (micro admissibility screening), `vet` (prima facie compliance vetting), and `audit` (substantive judicial adjudication)—against unified diff patches, workspace file snapshots, static `references:`, and filing metadata planes.

This document is the single authoritative specification for how exhibits are materialized, reconstructed against pre-patch workspaces, indexed with 1-based line coordinates, segmented into topological spans or Strategy D skeletons, bounded to prompt character ceilings, and rendered alongside filing metadata planes. Related contracts reside in:
- **[`nodes/intake.md`](nodes/intake.md):** Tendered exhibit ingestion, `FileArtifact` parsing, `--workspace-state` flag capture, and `Caseload` `.intake` schema.
- **[`nodes/configure.md`](nodes/configure.md):** User-level `~/.config/canon-clerk/config.json` `{ providers, defaults, nodes }` schema and runtime configuration hygiene.
- **[`providers.md`](providers.md):** `DecisionBudgetProfile` context/batching matrices governing discriminative slice packing in `admit` and `vet`.
- **[`github.md`](github.md):** Native `--github-pr` ingestion and the `pull_request_target` $\to$ `--workspace-state base` default heuristic.
- **[`nodes/admit.md`](nodes/admit.md), [`nodes/vet.md`](nodes/vet.md), and [`nodes/audit.md`](nodes/audit.md):** Stage-specific gating, scoring, fallback, and `CodeAnnotation` contracts.

---

## 2. Diff Exhibit Head-Context Budget

To eliminate **diff-keyhole blind spots** on small files and top-of-file preambles (license headers, YAML frontmatter, imports, `README.md` leads, and living-spec headers) when evaluating unified diffs (`--diff`, `--github-pr`), exhibit-rendering stages (`admit`, `vet`, `audit`) apply a shared, runtime-only **head-context budget** (`N` post-image lines) to `Modified`, `Renamed`, and `Copied` file exhibits:

1. **Configuration Cascade (Runtime-Only, Never Persisted on `Caseload`):**
   - **Precedence:** CLI `--diff-head-lines <LINES>` $\succ$ environment `CANON_CLERK_DIFF_HEAD_LINES` $\succ$ `~/.config/canon-clerk/config.json` `defaults.diffHeadLines` (alias `diff_head_lines`) $\succ$ built-in default `80`.
   - **Disable Mode:** Setting `0` disables head-context expansion completely and renders raw unified diff hunks verbatim. Invalid (negative or non-integer) values fail fast with exit code `2` and an actionable `Hint:`.
   - **CLI Surface:** Accepted on `admit`, `vet`, and `audit` (single shared threshold across all three stages).
2. **Unified Merge & Elision Semantics:**
   - **Small files ($\text{total\_lines} \le N$):** Head lines `1..=total_lines` merge with all diff hunks in the file, producing a complete post-patch annotated diff (`+`/`-` change markers and exact 1-based post-image line numbers preserved, zero duplicated lines).
   - **Large files ($\text{total\_lines} > N$):** Post-image lines `1..=N` are merged with any overlapping diff hunks; when the first hunk below line `N` starts at line $M > N + 1$, a single Strategy D-style elision marker (`// ... [L<N+1>..L<M-1> elided]`, or `<!-- ... [L<N+1>..L<M-1> elided] -->` for Markdown) separates the head prefix from the remaining `@@ ... @@` hunks.
   - **Minified-Line Safety Cap (`4_096` chars):** An internal `4_096`-character ceiling bounds the injected head-context prefix so minified single-line files never bloat prompts; once adding the next head line would exceed `4_096` characters, head-context injection stops (and if line `1` alone exceeds `4_096` characters, rendering falls back cleanly to the raw unified diff hunk).
   - **Downstream Compatibility:** Expanded 1-based exhibit line streams feed directly into `vet` topological span tiling (§4) and `audit` Strategy D breadcrumb skeletons (§4) with exact 1-based post-image line coordinates, while each stage continues enforcing its own per-exhibit character ceiling (§5).
3. **Post-Patch Reconstruction & Graceful Fallback:**
   - **No-op statuses:** `Added`, `Deleted`, binary, and patch-omitted exhibits are rendered unchanged.
   - **`--workspace-state head` (default):** Post-patch lines are read from the live workspace file on disk.
   - **`--workspace-state base` (e.g., `pull_request_target`):** Post-patch lines are reconstructed strictly in memory via the best-effort diff exhibit overlay (§3), never mixing unpatched base-branch lines into head-context exhibits.
   - **Graceful Fallback:** Missing workspace files, workspace-absent `--diff -` or `--caseload` replays, binary/non-UTF-8 files, or base-overlay patch mismatches fall back cleanly to the raw unified diff hunk with zero errors.

---

## 3. Workspace State & In-Memory Patch Overlay

The `.intake.workspace_state` flag (`"head"` default, or `"base"`) declares whether the checked-out workspace on disk already contains the diff's post-patch files (`head`) or represents the pre-patch base branch (`base`, such as GitHub Actions `pull_request_target` checkouts; see [`github.md` §3](github.md#3-pull-request-diff--metadata-ingestion---github-pr) for the automatic default heuristic).

Under `--workspace-state base`, Canon Clerk never mutates the workspace on disk and never leaks unpatched base-branch lines as post-patch evidence. Instead, it enforces two distinct in-memory overlay policies depending on whether the file is a **grounding reference** (`references:`) or a **diff exhibit** (head-context expansion):

1. **Fail-Closed `references:` Overlay (Statutory Grounding in `vet` and `audit`):**
   - Static frontmatter `references:` within a canon's monorepo `<scope>/` resolve against the reconstructed post-diff file tree by applying `.intake.diffs` in memory onto base-branch files via offset-tolerant exact-context matching.
   - Validation runs eagerly post-`validate` (0-token pre-flight) and idempotently prior to `vet` and `audit` on `--caseload` resumes.
   - Any missing base file, omitted patch (`binary`, `oversized`, `not_requested`) on a modified or added reference file, or unified diff hunk conflict fails fast with **exit code `2`** and an actionable remediation `Hint:`.
   - **Policy Security Boundary:** Declarative `<scope>/.canons/policy-chains.yaml` policy files remain read strictly from the trusted base checkout on disk and are never patched from untrusted diff streams ([`policy.md` §3](policy.md#3-scoped-discovery-containment--trust-boundaries), [`SPEC.md` §7](../../SPEC.md#7-workspace-policy-chains-canonspolicy-chainsyaml)).
2. **Best-Effort Diff Exhibit Overlay (Head-Context Expansion in `admit`, `vet`, and `audit`):**
   - For `Modified`, `Renamed`, and `Copied` exhibits, post-patch head lines are reconstructed strictly in memory by applying `.intake.diffs` onto the base-branch file (`previous_path` for `Renamed`/`Copied`).
   - Unlike statutory `references:`, any missing base file, non-UTF-8/binary payload, or patch mismatch falls back cleanly to raw unified diff hunks without aborting evaluation and without exposing unpatched base-branch lines.

| File Change Status | Fail-Closed `references:` Overlay (`vet`, `audit`) | Best-Effort Diff Head-Context Overlay (`admit`, `vet`, `audit`) |
| :--- | :--- | :--- |
| **`Added`** | Reconstructed in memory from the unified diff patch (fails fast with exit `2` if patch is omitted or invalid). | No-op (diff patch already contains the complete added file). |
| **`Modified`** | Base file at `path` patched in memory with `.intake.diffs[path]` (fails fast with exit `2` on missing base file, omitted patch, or conflict). | Base file at `path` patched in memory to derive post-image head lines; falls back to raw hunks on any error. |
| **`Renamed` / `Copied`** | Base file at `previous_path` copied and patched in memory if modified (100%-similarity `unchanged` renames/copies preserve base content directly); fails fast with exit `2` on missing `previous_path` or patch conflict. | Base file at `previous_path` patched in memory to derive post-image head lines; falls back to raw hunks on any error. |
| **`Deleted`** | Excluded from the reconstructed post-diff reference tree. | No-op (rendered verbatim as unified deletion hunks). |
| **`Unchanged`** | Read directly from the base workspace snapshot on disk. | No-op (no patch hunks to expand). |

---

## 4. 1-Based Line Coordinates, Span Tiling & Strategy D Skeletons

### Unified 1-Based Exhibit Coordinate Model
Both unified diff patches (after optional head-context expansion per §2) and full-file workspace snapshots materialize into a unified sequence of 1-based target-file exhibit lines:
- **Post-Image Line Numbering:** Context (` `) and added (`+`) diff lines, as well as full-file snapshot lines, carry their exact 1-based post-image target-file line number (`L >= 1`). Deleted (`-`) diff lines and `@@ ... @@` hunk headers remain anchored to their surrounding hunk without advancing post-image line coordinates.
- **Post-Image Structural Tracking:** Indentation depth, enclosing scopes, Markdown heading levels, and configuration section headers are tracked strictly on post-image lines, ignoring deleted diff lines so removed code never corrupts structural nesting.
- **Deleted-Only Hunk Anchoring:** Spans containing solely deletions anchor to the hunk's post-image start line (`L..L`, clamped to `>= 1`).

### Pan-Source Topological Span Tiling (`vet`)
Before packing discriminative `state` slices, `vet` segments each untruncated file exhibit into bounded topological spans:
- **Target Span Geometry:** Files within the single-span ceiling (`<= 65` lines) remain a single span. Larger files are cut into ~`40`–`65` line spans (targeting ~`52` lines) aligned to natural structural boundaries: blank lines, indentation dedents, Markdown `#` headings, and TOML/configuration `[section]` headers.
- **Preamble Glue:** Contiguous declaration preambles (attributes, decorators, and doc comments) stay attached to the declaration immediately below them rather than stranding across a span cut.
- **Unspaced-Block Fallback:** When a dense block lacks natural structural boundaries within the target window, tiling falls back to fixed-size cuts with a `5`-line overlap so boundary-straddling constructs remain visible.
- **Lexical Span Fences & Adaptive Fallback:** When a multi-span file retains span-level localization within the active model's `DecisionBudgetProfile` ([`providers.md` §4](providers.md#4-decisionbudgetprofile--per-profile-execution-defaults)), its spans are delimited with explicit lexical span fences inside `state`; when a slice falls back to file-level vetting, lexical span fences are omitted so `state` renders clean unfenced file blocks without unused fence overhead.

### Strategy D Breadcrumb Skeletons (`audit`)
When `.vetting.cases[canon_path].contested_spans` localizes one or more hot spans for an evaluated file exhibit, `audit` renders that file as a **Strategy D breadcrumb skeleton** purely from the exhibit's 1-based line stream:
- **Retained Regions:**
  1. **Hot Contested Spans:** Every line inside a localized `contested_spans` range is expanded verbatim.
  2. **Top-of-File Preamble:** Bounded top-of-file imports/headers (code: up to `40` lines; Markdown frontmatter/lead: up to `30` lines; configuration headers: up to `20` lines).
  3. **Structural Ancestor Breadcrumbs:** Enclosing code ancestor scopes (with their attached attributes, decorators, or doc comments), fence-aware Markdown heading stacks, configuration `[section]` headers, and enclosing `@@ ... @@` unified diff hunk headers.
- **Cold-Region Elision:** Contiguous non-retained cold regions collapse into a single line-range elision marker (`// ... [L<start>..L<end> elided]`, or `<!-- ... [L<start>..L<end> elided] -->` for Markdown) while preserving exact 1-based target-file line numbers for `CodeAnnotation` anchoring.
- **Ordering & Fallback:** Strategy D elision runs *before* enforcing `audit`'s per-exhibit character ceiling (§5). Files without entries in `contested_spans` fall back to full exhibit rendering.

---

## 5. Per-Exhibit Bounds & Metadata Plane Rendering

### Per-Stage Exhibit & Reference Character Ceilings
Each exhibit-consuming stage enforces deterministic character budgets at valid Unicode scalar boundaries, appending explicit truncation markers whenever content exceeds its ceiling:

| Stage | Per-Exhibit Ceiling | Grounding Reference Ceiling | Head-Context Prefix Cap | Slice / Batch Budget |
| :--- | :--- | :--- | :--- | :--- |
| **`admit`** | `8_192` chars per candidate file or metadata exhibit | *(not injected; `admit` screens filing exhibits only)* | `4_096` chars (§2) | Governed by active model's `DecisionBudgetProfile` ([`providers.md` §4](providers.md#4-decisionbudgetprofile--per-profile-execution-defaults)) |
| **`vet`** | Bounded by active slice budget (untruncated exhibits feed span tiling first) | Static `<scope>/` `references:` repeated per slice (pinned diffs for admitted referenced companions) | `4_096` chars (§2) | Governed by active model's `DecisionBudgetProfile` ([`providers.md` §4](providers.md#4-decisionbudgetprofile--per-profile-execution-defaults)) |
| **`audit`** | `24_576` chars per evaluated file or metadata exhibit (`12_288` chars per linked issue body) | `12_288` chars per `<scope>/` `references:` file (pinned diffs for admitted referenced companions) | `4_096` chars (§2) | `60_000` chars per trial slice (oversized single exhibits are admitted as a singleton slice so no contested exhibit is dropped) |

### Filing Metadata Plane Rendering
When a canon's `inspect:` frontmatter declares a filing metadata plane and the corresponding `.intake` field is non-empty, the plane renders as a first-class exhibit alongside file exhibits (adhering to [`SPEC.md` §4.2](../../SPEC.md#42-deterministic-derivation-rules)):
- **`pr_title`:** Literal non-blank pull request title text.
- **`pr_body`:** Non-blank pull request description bounded to the stage's per-exhibit character ceiling.
- **`commit_messages`:** Chronologically numbered list (`1.`, `2.`, ...) preserving ingestion order (never sorted lexicographically), prefixing each entry with an optional 7-character short SHA (`[<short_sha>]`) and indented continuation lines for multi-line commit bodies. Capped by both item count (up to `50` commits in `vet`/`audit`) and total character ceiling, emitting an explicit omission summary (`... (+N additional commit(s))`) when truncated. Excludes commit diffs/stats, tree/parent hashes, signatures, timestamps, and author/committer emails.
- **`linked_issues`:** Structured entries recording `#<number>`, title, optional author (`@<login>`) and comma-separated labels, and bounded Markdown body (up to half the stage's per-exhibit ceiling per issue in `vet`/`audit`), excluding issue comments and reaction trails.

### Filing-Wide Admitted Path Manifest
Both `vet` and `audit` attach the complete filing-wide list of all admitted file exhibit paths (`All Admitted File Exhibit Paths in Filing`) to every case prompt and every multi-file slice so companion-update invariants never false-positive when exhibits are partitioned across slices or pruned via `contested_exhibits`.
