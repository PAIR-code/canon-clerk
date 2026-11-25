# GitHub Platform Adapter: PR & Issue Ingestion & Checks Publication

**Status:** Authoritative Architectural Standard  
**Domain Concept:** Stage-Agnostic Pull Request & Issue Inquiry Ingestion, Outbound Checks Publication, Target Context Resolution & CI Forensics Durability

---

## 1. Executive Summary & Architectural Role

GitHub Actions workflow command annotations (`::error::`, `::warning::`, `::notice::`) enforce a hard platform ceiling of **10 errors and 10 warnings per step** (50 annotations total per job). On pull requests with more than 10 canon violations, findings beyond the limit are silently dropped from the pull request **Files changed** diff and Checks UI. Likewise, relying on shell wrappers (`jq`, `gh`) to extract pull request diffs and metadata duplicates GitHub context parsing outside the binary and complicates standalone GitHub Action adoption.

Canon Clerk treats forge integrations as **driven adapters at the boundary of the hexagon** (`overview.md` §3)—parallel to outbound model providers (`transport.md`)—so stage engines (`intake`, `audit`, `vet`, etc.) remain completely decoupled from GitHub wire protocols and environment conventions.

```mermaid
flowchart LR
    subgraph Stages ["Caseload Evaluation Stages"]
        Intake["intake<br/>(Pure In-Memory Filing)"]
        Audit["audit<br/>(Verdict, Static Validation &amp; Skip)"]
    end

    subgraph CliBoundary ["CLI Driving Adapter"]
        PrFlags["PR Filing Flags<br/>(--github-pr [&lt;NUMBER&gt;],<br/>--github-repo, --linked-issue,<br/>--commit-message, --commit-messages-file)"]
        CheckFlags["GitHub Check Flags<br/>(--publish-github-check,<br/>--github-head-sha, --github-check-name,<br/>--github-bypass-label [&lt;LABEL&gt;])"]
        Forensics["Forensics Flush<br/>(--output-caseload)"]
    end

    subgraph GithubAdapter ["Driven GitHub Adapter"]
        Context["Context &amp; Credential Resolver<br/>(GITHUB_TOKEN / GH_TOKEN,<br/>GITHUB_REPOSITORY, GITHUB_EVENT_PATH,<br/>GITHUB_EVENT_NAME, GITHUB_SHA, GITHUB_API_URL)"]
        PrFetcher["Pull Request, Commit &amp; Issue Fetcher<br/>(Event Payload Parser, PR Metadata JSON,<br/>Unified Diff, Paginated Commits &amp; Linked Issues)"]
        Publisher["Stage-Agnostic Check Run Publisher<br/>(Annotation Sanitizer, 65,535-Char Guards,<br/>POST + 50-Annotation PATCH Batching)"]
    end

    subgraph GitHub ["GitHub REST API"]
        Pulls["Pulls &amp; Issues API<br/>(/repos/{owner}/{repo}/pulls/{number},<br/>/pulls/{number}/commits,<br/>/repos/{owner}/{repo}/issues/{number})"]
        Checks["Checks API<br/>(/repos/{owner}/{repo}/check-runs)"]
    end

    PrFlags --> Context
    CheckFlags --> Context
    Context --> PrFetcher --> Pulls
    PrFetcher --> Intake
    Audit --> Forensics --> Publisher --> Checks
    Context --> Publisher
```

---

## 2. Target Context Resolution & Pre-Flight Fail-Fast

GitHub PR ingestion (`--github-pr [<NUMBER>]`), GitHub Issue inquiry ingestion (`--github-issue [<NUMBER>]`), and GitHub Checks publication (`--publish-github-check`) are strictly **opt-in** and never fire implicitly—even when `GITHUB_ACTIONS=true`.

When any of these options is enabled, Canon Clerk resolves and validates the required GitHub context **up front before any model spend occurs**, failing fast with exit code `2` and an actionable remediation `Hint:` if any required input is missing or malformed. Target resolution executes independently of Branch B `configure` so that zero-candidate short-circuits, static validation failures, credential-skip exits (`--on-missing-credentials skip`), and offline `--caseload` replays all validate and publish consistently.

| Parameter | Resolution Precedence | Validation & Security Constraints |
| :--- | :--- | :--- |
| **Auth Token** | `GITHUB_TOKEN` $\to$ `GH_TOKEN` | Read strictly from environment variables; **never** accepted as a CLI flag and redacted from debug/diagnostic output. Requires `pull-requests: read` for PR ingestion, `issues: read` for numbered `--github-issue <NUMBER>` or linked-issue fetching on private/restricted repos, and `checks: write` for Check Run publication. Not required for bare `--github-issue` (zero-API-read from `$GITHUB_EVENT_PATH`). |
| **Repository** | `--github-repo <OWNER/REPO>` $\to$ `GITHUB_REPOSITORY` | Shared across filing ingestion (`FilingSourceArgs`) and Check Run publication without flag collisions. Must be a valid `<owner>/<repo>` slug (two non-empty path segments, no whitespace or traversal). Not required for bare `--github-issue`. |
| **Pull Request Number** | `--github-pr <NUMBER>` $\to$ `pull_request.number` in `GITHUB_EVENT_PATH` | Required when `--github-pr` is active; must be a positive integer (`>= 1`). |
| **Issue Number** | `--github-issue <NUMBER>` $\to$ `issue.number` in `GITHUB_EVENT_PATH` | Required when `--github-issue` is active (scoped to `intake` and `apprise`); must be a positive integer (`>= 1`). |
| **Head Commit SHA** | `--github-head-sha <SHA>` $\to$ `pull_request.head.sha` in `GITHUB_EVENT_PATH` $\to$ fetched PR API `head.sha` (when `--github-pr` is active) $\to$ `GITHUB_SHA` | Preferring `pull_request.head.sha` over `GITHUB_SHA` ensures triangular fork PRs running under `pull_request_target` annotate the pull request's head commit rather than the base branch; falling back to the fetched PR API `head.sha` enables `--github-pr <NUMBER> --publish-github-check` outside GitHub Actions. |
| **API Base URL** | `GITHUB_API_URL` $\to$ `https://api.github.com` | Supports GitHub Enterprise Server deployments and hermetic loopback mock servers. |
| **Check Run Name** | `--github-check-name <NAME>` $\to$ `"Canon Clerk Audit"` | Distinct from the Actions job name to avoid Check Run collisions. |
| **Bypass Label** | `--github-bypass-label [<LABEL>]` $\to$ `CANON_CLERK_GITHUB_BYPASS_LABEL` | Strictly opt-in (disabled when omitted); bare `--github-bypass-label` defaults to `"skip-audit"`. Must be non-empty when explicitly passed; matched case-insensitively against `pull_request.labels[].name`. |

---

## 3. Pull Request Diff & Metadata Ingestion (`--github-pr`)

The CLI driving adapter supports native pull request ingestion via `--github-pr [<NUMBER>]` across `intake` and every cascading stage (`discover`, `validate`, `apprise`, `docket`, `admit`, `vet`, `audit`), while keeping the `intake` core domain engine (`src/intake/`) 100% pure in-memory with zero network or subprocess side effects:

1. **Metadata Resolution:**
   - **Bare `--github-pr`:** Reads `pull_request.number`, `pull_request.title`, `pull_request.body`, `pull_request.head.sha`, `pull_request.labels[].name`, and event `sender.login` directly from the JSON file at `$GITHUB_EVENT_PATH`. Unreadable or malformed event payloads, or payloads missing `pull_request.number`, fail fast with exit code `2` and an actionable `Hint:`.
   - **Explicit `--github-pr <NUMBER>`:** Fetches pull request metadata (`title`, `body`, `head.sha`, `labels[].name`) via `GET /repos/{owner}/{repo}/pulls/{pull_number}` with `Accept: application/vnd.github+json`.
2. **Unified Diff Fetching:**
   - Fetches the pull request unified diff directly via `GET /repos/{owner}/{repo}/pulls/{pull_number}` with `Accept: application/vnd.github.v3.diff` over Canon Clerk's persistent background HTTP reactor (`transport.md` §2), setting `X-GitHub-Api-Version: 2022-11-28`, `User-Agent: canon-clerk/<version>`, and `Authorization: Bearer <token>`.
3. **Linked-Issue Discovery & Fetching (`linked_issues`):**
   - Unless overridden by explicit `--linked-issue <PATH|->` CLI flags, Canon Clerk scans the resolved `pr_body` (after `--pr-body` / `--pr-body-file` override resolution) for same-repo closing keywords matching `SPEC.md` §4.2's case-insensitive pattern `(close[sd]?|fix(e[sd])?|resolve[sd]?)\s+#(\d+)`, deduplicated in deterministic ascending issue-number order. (GraphQL `closingIssuesReferences` is not used by the REST adapter.)
   - For each discovered issue number, the adapter fetches `GET /repos/{owner}/{repo}/issues/{issue_number}` with `Accept: application/vnd.github+json` and projects strictly `number`, `title`, `user.login` (`author`), `labels[].name` (`labels`), `body`, and `html_url` (`url`) onto `LinkedIssueContext`—excluding issue comments and reaction trails per `SPEC.md` §4.2.
   - **Graceful Degradation:** Pull requests returned by the GitHub Issues endpoint (responses containing a `"pull_request"` key), HTTP `404` / `403` responses (e.g., missing `issues: read` permission), and transient per-issue fetch errors are skipped with diagnostic warnings rather than aborting the audit.
4. **Commit Message Ingestion (`commit_messages`):**
   - Unless overridden by explicit `--commit-message <TEXT>` or `--commit-messages-file <PATH|->` CLI flags, Canon Clerk fetches pull request commits via `GET /repos/{owner}/{repo}/pulls/{pull_number}/commits?per_page=100` (`Accept: application/vnd.github+json`), projecting strictly `sha` and `commit.message` (excluding commit diffs/stats, tree/parent hashes, signatures/verification, timestamps, and author/committer emails per `SPEC.md` §4.2).
   - Follows RFC 8288 `Link: rel="next"` pagination deterministically up to a cap of **3 pages / 250 commits** (matching GitHub's PR commits ceiling) while preserving API chronological order; if truncated at the cap, emits a diagnostic warning to `stderr` rather than omitting commits silently.
   - **Fail-Fast Error Semantics:** Unlike optional cross-repo `linked_issues` `403`/`404` degradation, `/pulls/{pull_number}/commits` uses the same `pull-requests: read` permission as PR metadata and unified diffs, and failing open with empty commits would silently prune all `commit_messages` canons (`discover` false green). HTTP and transport failures on `/pulls/{pull_number}/commits` MUST fail fast with exit code `2` and an actionable `Hint:`.
5. **Flag Composition & Precedence:**
   - `--github-pr` conflicts with `--diff` (mutually exclusive patch sources).
   - Explicit `--pr-title`, `--pr-body`, `--pr-body-file`, `--linked-issue`, `--commit-message`, and `--commit-messages-file` always override metadata fetched from `$GITHUB_EVENT_PATH` or the GitHub API (`CLI flag > event/API metadata`). When explicit `--linked-issue` flags are supplied, GitHub Issues API fetches are skipped; when explicit `--commit-message` or `--commit-messages-file` flags are supplied, GitHub PR Commits API fetches are skipped.
   - **Workspace State Heuristic:** When `--github-pr` is active and `--workspace-state` is omitted, `--workspace-state` defaults to `base` **iff** `GITHUB_EVENT_NAME == "pull_request_target"`; otherwise it preserves the default `head`. An explicit `--workspace-state <head|base>` always overrides.

---

## 4. Stage-Agnostic Check Run Publication & Wire Invariants

Stages translate their domain outcomes into a stage-agnostic Check Run report consisting of a check name, terminal conclusion (`success`, `failure`, `neutral`, or `skipped`), headline title, Markdown summary, optional Markdown detail text, and flat list of source annotations.

The outbound GitHub Checks client enforces the following wire invariants over Canon Clerk's persistent background HTTP reactor (`transport.md` §2):

1. **Batched Creation & Continuation (`POST` + 50-Annotation `PATCH`):**  
   - Creates the completed Check Run via `POST /repos/{owner}/{repo}/check-runs` with the first batch of up to **50** annotations.
   - Streams all remaining annotations in consecutive batches of **50** via `PATCH /repos/{owner}/{repo}/check-runs/{check_run_id}` until every finding is published, carrying `output.title`, `output.summary`, and `output.text` on each update as required by GitHub's Checks API.
   - Sets required GitHub REST API headers (`Accept: application/vnd.github+json`, `X-GitHub-Api-Version: 2022-11-28`, `User-Agent: canon-clerk/<version>`, and `Authorization: Bearer <token>`).
2. **Annotation Span Sanitization (HTTP 422 Prevention):**  
   - Normalizes line bounds to 1-based `start_line >= 1` and `end_line >= start_line`.
   - Preserves 1-based `start_column` / `end_column` strictly on single-line spans (`start_line == end_line`) and omits both column fields whenever `start_line != end_line`, preventing GitHub API HTTP `422 Unprocessable Entity` rejections on multi-line annotations.
3. **Output Character-Budget Guards (65,535-Character Ceiling):**  
   - Both `output.summary` and `output.text` are bounded to GitHub's 65,535-character limit at valid Unicode scalar boundaries, appending an explicit truncation notice whenever content exceeds the ceiling so findings are never silently cut off.

---

## 5. CLI Ergonomics, Forensics Durability & Replay

1. **Duplicate Annotation Suppression & Step Summary Parity:**  
   When `--publish-github-check` is active and neither `--format` nor `--json` is explicitly specified under `GITHUB_ACTIONS=true`, terminal `stdout` defaults to `stylish` rather than `github`. This prevents GitHub Actions from rendering duplicate `::error::` workflow commands alongside the Check Run's inline annotations, while still appending the rendered Markdown Funnel Provenance Receipt and Verdict Report to `$GITHUB_STEP_SUMMARY` when set so the Actions workflow run summary page displays full results alongside the standalone Check Run. Explicit `--format github` remains available for zero-setup workflows that do not grant `checks: write`.
2. **Forensics-First Durability:**  
   `--output-caseload <PATH>` is always flushed to disk **before** issuing any GitHub Checks API request. If GitHub returns a transient HTTP or transport error, the CLI exits with code `2` and prints an actionable replay hint referencing the saved Caseload artifact so forensics and LLM results are never lost.
3. **Zero-Spend Replay:**  
   Because terminal stages fast-forward pre-populated Caseload records, invoking `canon-clerk audit --caseload <PATH> --publish-github-check` replays publication directly from a recorded Caseload artifact with zero LLM spend.

---

## 6. Pull Request Label Bypass & Collaborator Security Boundary

When `Canon Clerk Audit` is promoted from advisory to an enforcing required status check, maintainers need a lightweight, auditable escape hatch for LLM false positives, emergency hotfixes, and pull requests that delete or revise faulty canons (which `pull_request_target` otherwise evaluates against `main`'s older `.canons/` corpus).

1. **Opt-In Configuration & Label Resolution:**  
   Bypass evaluation is strictly **opt-in** via `--github-bypass-label [<LABEL>]` (bare flag defaults to `"skip-audit"`) or the `CANON_CLERK_GITHUB_BYPASS_LABEL` environment variable (`CLI flag > env var`). When active, Canon Clerk inspects pull request labels from the resolved `--github-pr` metadata (`$GITHUB_EVENT_PATH` or Pulls API) or, when `--github-pr` is inactive, from `pull_request.labels[].name` in `$GITHUB_EVENT_PATH`. Label comparison is case-insensitive to match GitHub's label semantics.
2. **Zero-Spend Short-Circuit & `skipped` Check Run Publication:**  
   When the configured bypass label is present on the pull request, `audit` short-circuits immediately after pre-flight target validation—before `discover`, `validate`, `configure`, and any model calls (0 tokens, 0 provider requests, and independent of provider API credentials). Canon Clerk flushes `--output-caseload` (when configured), writes an explicit bypass notice to stdout and `$GITHUB_STEP_SUMMARY`, publishes a GitHub Check Run with `conclusion: "skipped"` (which GitHub treats as non-blocking for required status checks) recording the matched bypass label and triggering actor (`GITHUB_ACTOR` or event `sender.login`) in `output.summary`, and exits with code `0`.
3. **Collaborator-Only Security Boundary & Workflow Lifecycle:**  
   On GitHub, external fork contributors cannot apply or remove pull request labels; only repository collaborators with `triage` or `write`+ permissions can trigger the bypass, and GitHub's pull request timeline permanently audits who applied the label. Workflows enabling `--github-bypass-label` SHOULD subscribe to both `labeled` and `unlabeled` pull request event types so removing the bypass label immediately re-runs the audit and replaces the `skipped` Check Run on the head commit.

---

## 7. GitHub Issue Inquiry Ingestion (`--github-issue`)

For the prospective design track (`intake` and `apprise`), the CLI driving adapter supports opt-in GitHub Issue ingestion via `--github-issue [<NUMBER>]` while keeping `src/intake/` 100% pure in-memory with zero network or subprocess side effects:

1. **Resolution Modes (Zero-API Event vs. Fail-Fast REST):**
   - **Bare `--github-issue`:** Reads `issue.number`, `issue.title`, `issue.body`, `issue.labels[].name`, `issue.user.login`, and `issue.html_url` directly from the JSON file at `$GITHUB_EVENT_PATH`, enabling zero-API-read GitHub Actions workflows triggered on `issues: [opened, edited, labeled, reopened]` without requiring `GITHUB_TOKEN` or `GITHUB_REPOSITORY`. Fails fast with exit code `2` and an actionable `Hint:` if `$GITHUB_EVENT_PATH` is missing, unreadable, malformed, or lacks a positive integer `issue.number` (`>= 1`).
   - **Numbered `--github-issue <NUMBER>`:** Validates `<NUMBER>` as a positive integer (`>= 1`), requires `GITHUB_TOKEN` / `GH_TOKEN` and `--github-repo <OWNER/REPO>` / `GITHUB_REPOSITORY`, and fetches `GET /repos/{owner}/{repo}/issues/{number}` with `Accept: application/vnd.github+json` over the persistent HTTP reactor (`GITHUB_API_URL` override supported). Unlike linked-issue discovery's graceful degradation on `--github-pr`, numbered `--github-issue <NUMBER>` **fails fast with exit code `2`** and an actionable `Hint:` on HTTP `404`, `403`, any other non-2xx status, transport/protocol failures, or when the returned JSON contains a non-null `"pull_request"` key (rejecting pull requests passed as issues).
   - **Comments & Reactions Excluded:** Consistent with `SPEC.md` §4.2's noise and trust boundary, issue comments and reaction trails are never ingested.
2. **Caseload Mapping & Precedence:**
   - Formats the resolved issue deterministically into `.intake.intent`: `#<number>: <title>` (falling back to `#<number>` if title is absent), followed by `Labels: <label1>, <label2>` when non-empty, followed by a blank line and the Markdown `body` when non-empty.
   - Explicit `--intent <TEXT>` always overrides the formatted issue text (`CLI flag > event/API metadata`, matching `--pr-title`/`--pr-body` precedence on `--github-pr`).
   - Does **not** populate `.intake.linked_issues`, preserving `SPEC.md` §4.2's PR-linked-issue metadata plane semantics so replaying an issue intake `Caseload` never falsely activates `inspect: [linked_issues]` canons on the contentious track.
3. **Stage Scoping & Conflict Guards:**
   - **Exposed only on `intake` and `apprise`:** `--github-issue` appears in CLI `--help` strictly on `intake` and `apprise`.
   - **Self-Correcting Rejection Elsewhere:** Passing `--github-issue` on contentious-track stages (`docket`, `admit`, `vet`, `audit`) fails fast with exit code `2` and a `Hint:` pointing to `--linked-issue <PATH|->` for PR/diff audits (or `canon-clerk apprise --github-issue` for prospective issue screening). Passing `--github-issue` on `discover` or `validate` fails fast with exit code `2` and a `Hint:` pointing to `canon-clerk apprise --github-issue` or piping `canon-clerk intake --github-issue ... --json | canon-clerk <stage> --caseload -`.
   - **Mutually Exclusive with `--github-pr`:** Passing both `--github-issue` and `--github-pr` fails fast with exit code `2`. Composes cleanly with positional `TARGETS`, `--canon`, and `apprise`'s default `--all-canons` fallback.
