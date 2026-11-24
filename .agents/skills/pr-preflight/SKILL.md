---
name: pr-preflight
description: >-
  Runs Canon Clerk's shift-left pre-flight verification gauntlet before pushing
  or opening a pull request: validates PR titles against pr-title-lint.yml,
  executes offline CI parity checks (license, file size, canon taxonomy, fmt,
  clippy, tests, validate), runs conditional workflow/metadata checks, and runs
  live canon-clerk audit with PR metadata forwarded. Invoke when prompted to:
  "Run pre-flight checks", "Verify before PR", "Check PR title", "Audit my WIP
  changes", or "Ready to open PR?".
---

# PR Pre-Flight Skill

Executes the local shift-left verification pipeline inside an active feature worktree prior to pushing or opening a pull request.

---

## Companion Script

The composite runner resides in `.agents/skills/pr-preflight/scripts/preflight.sh` and must be executed from inside a worktree checkout (not the workspace super-root).

### `preflight.sh [options] [-- <extra audit args>]`

Executes in fail-fast order:
1. Validates `--pr-title` (when provided) via `./scripts/check-pr-title.sh` against `.github/workflows/pr-title-lint.yml`.
2. Runs `./scripts/check.sh` (license headers, file-size bounds, canon taxonomy, `cargo fmt --check`, `cargo clippy`, `cargo test`, `validate --all-canons --max-warnings 0`).
3. Runs conditional checks when matching paths appear in the diff (`python3 .github/scripts/repo_settings.py check` for `.github/settings.yml`; `zizmor` for `.github/workflows/*.yml` when installed).
4. Runs live `canon-clerk audit --diff -` against the `upstream/main` merge-base with `--pr-title` and `--pr-body` / `--pr-body-file` forwarded.

```bash
# Full pre-flight with PR title validation and live audit:
./.agents/skills/pr-preflight/scripts/preflight.sh \
  --pr-title "feat(cli): add output format flag (#123)"

# Include drafted PR body so PR-description canons evaluate locally:
./.agents/skills/pr-preflight/scripts/preflight.sh \
  --pr-title "feat(cli): add output format flag (#123)" \
  --pr-body-file /tmp/pr-body.md

# Forward extra arguments directly to `canon-clerk audit`:
./.agents/skills/pr-preflight/scripts/preflight.sh \
  --pr-title "feat(cli): add output format flag (#123)" \
  -- --provider google

# Inner-loop deterministic checks only (NOT a substitute for pre-PR live audit):
./.agents/skills/pr-preflight/scripts/preflight.sh \
  --pr-title "feat(cli): add output format flag (#123)" \
  --no-audit
```

### Standalone PR Title Check

To validate a candidate PR title alone before calling `gh pr create` or `gh pr edit`:

```bash
./scripts/check-pr-title.sh "feat(spec,canons): introduce Precedent directive (#420)"
```

---

## Operational Invariants

1. **Run the Full Pre-Flight (Including Live Audit) Before Pushing or Opening a PR:** Always execute `preflight.sh` *without* `--no-audit` before pushing or creating a pull request so semantic canon violations are caught locally rather than in CI. Reserve `--no-audit` strictly for fast inner-loop iteration on deterministic checks.
2. **Stage or Commit New Files First:** `preflight.sh` diffs tracked files against the `upstream/main` merge-base (`git diff <merge-base>`). Stage new files with `git add` (or commit them) before running `preflight.sh` so they are included in the audit diff.
3. **Always Pass `--pr-title` Before Opening a PR:** Passing `--pr-title` catches invalid Conventional Commit types/scopes locally and prevents PR-title canons from being pruned during `canon-clerk audit`.
4. **Inspect Shunted Logs on Failure:** Output exceeding 8KB is automatically shunted to `/tmp/pr-preflight-<timestamp>.log`; inspect the full file rather than re-running individual commands blindly.
5. **PR Body Hygiene:** Before running `gh pr create`, ensure the PR body omits CI checklists and file-by-file diff narration.
