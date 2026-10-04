# @canon-clerk/repo-settings-private

Internal workspace tool for validating and synchronizing declarative repository metadata (description, homepage, topics) from `.github/settings.yml` to GitHub using the native GitHub CLI (`gh`).

---

## 1. Overview

Repository metadata in GitHub is commonly configured manually via the web interface. To bring this under version control, peer review, and automation without relying on prohibited third-party GitHub Apps (such as Probot), `@canon-clerk/repo-settings-private` provides:

* **Declarative Schema Validation:** Validates `.github/settings.yml` syntax and GitHub topic restrictions.
* **Diff Computation:** Detects differences between local desired settings and live GitHub repository metadata.
* **Native Synchronization:** Applies changes directly through `gh repo edit` without bespoke HTTP clients or external third-party dependencies.

---

## 2. Local Usage

From anywhere in the monorepo:

### Validate Syntax & Dry-Run Diff
Inspect changes and verify that `.github/settings.yml` is valid against live GitHub state:

```bash
npm run settings:check
```

Example output:
```text
Loading settings from: /path/to/.github/settings.yml
✓ Settings YAML syntax and schema validated successfully.
Target repository: PAIR-code/canon-clerk

Repository settings diff for 'PAIR-code/canon-clerk':
  Description:
    - current: "An open specification for project canons..."
    + desired: "Semantic linter, standard rulepack catalog, and open specification for project canons."
  Homepage:
    - current: ""
    + desired: "https://github.com/PAIR-code/canon-clerk"
  Topics to add (+):
    + semantic-linter
    + rule-packs
    + architectural-linter

Dry-run completed: Changes are pending synchronization.
```

### Apply Settings Manually
If you have repository administrator access in your local `gh` session, you can apply settings directly:

```bash
npm run settings:sync
```

---

## 3. GitHub Actions CI & Out-of-Band Setup

Automated synchronization is handled by `.github/workflows/repo-metadata.yml`:
* **Pull Requests:** Runs `settings:check` (dry run) so reviewers can see the planned metadata diff in CI logs.
* **Push to `main`:** Runs `settings:sync` to automatically update GitHub metadata on merge.

### Why `REPO_SETTINGS_TOKEN` Is Required

GitHub's built-in `GITHUB_TOKEN` intentionally **does not provide repository administration permissions** (`administration: write`). Updating repository descriptions, homepages, and topics via GitHub's REST API requires administrator privileges.

To enable automated synchronization on merge to `main`, a maintainer must perform a one-time out-of-band setup of `REPO_SETTINGS_TOKEN`.

---

## 4. One-Time Setup: Configuring `REPO_SETTINGS_TOKEN`

Follow these steps to create and configure the token in GitHub:

### Step 1: Generate a Fine-Grained Personal Access Token (PAT)
1. Go to GitHub: **Settings** (your personal user profile) → **Developer settings** → **Personal access tokens** → **Fine-grained tokens**.
   * Direct URL: `https://github.com/settings/tokens?type=beta`
2. Click **Generate new token**.
3. Configure the token details:
   * **Token name:** `canon-clerk-repo-settings`
   * **Expiration:** Set desired rotation interval (e.g. 90 days or 1 year per organization policy).
   * **Resource owner:** `PAIR-code`
   * **Repository access:** Choose **Only select repositories** and select `PAIR-code/canon-clerk`.
4. Configure Permissions:
   * Under **Repository permissions**, find **Administration**.
   * Change access to: **Read and write**.
5. Click **Generate token** and copy the resulting `github_pat_...` string.

### Step 2: Add Secret to `PAIR-code/canon-clerk`
1. Navigate to the canonical repository: `https://github.com/PAIR-code/canon-clerk`
2. Go to **Settings** → **Secrets and variables** → **Actions**.
3. Click **New repository secret**.
4. Configure:
   * **Name:** `REPO_SETTINGS_TOKEN`
   * **Secret:** Paste the copied token value.
5. Click **Add secret**.

Once added, any push to `main` modifying `.github/settings.yml` will automatically synchronize repository metadata to GitHub.
