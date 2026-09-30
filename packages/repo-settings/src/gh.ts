import { execFileSync } from "node:child_process";
import type { RemoteRepoMetadata, RepoSettingsFile, SettingsDiff } from "./types.js";

export type ExecFunction = (command: string, args: string[]) => string;

export const defaultExec: ExecFunction = (command: string, args: string[]) => {
  return execFileSync(command, args, { encoding: "utf-8", stdio: ["ignore", "pipe", "pipe"] });
};

/**
 * Resolves the target GitHub repository (e.g. "PAIR-code/canon-clerk").
 */
export function resolveTargetRepo(specifiedRepo?: string, execCmd: ExecFunction = defaultExec): string {
  if (specifiedRepo && specifiedRepo.trim()) {
    return specifiedRepo.trim();
  }

  // Attempt to resolve from git remote
  for (const remoteName of ["upstream", "origin"]) {
    try {
      const url = execCmd("git", ["config", "--get", `remote.${remoteName}.url`]).trim();
      const match = url.match(/(?:github\.com[:/])([^/]+)\/([^/.]+?)(?:\.git)?$/);
      if (match) {
        return `${match[1]}/${match[2]}`;
      }
    } catch {
      // Continue to next remote or fallback
    }
  }

  return "PAIR-code/canon-clerk";
}

/**
 * Fetches current remote repository metadata via GitHub CLI (`gh repo view`).
 */
export function fetchRemoteMetadata(
  repo: string,
  execCmd: ExecFunction = defaultExec
): RemoteRepoMetadata {
  try {
    const output = execCmd("gh", [
      "repo",
      "view",
      repo,
      "--json",
      "description,homepageUrl,repositoryTopics",
    ]);
    const parsed = JSON.parse(output) as RemoteRepoMetadata;
    return {
      description: parsed.description ?? "",
      homepageUrl: parsed.homepageUrl ?? "",
      repositoryTopics: parsed.repositoryTopics ?? [],
    };
  } catch (err) {
    throw new Error(
      `Failed to query repository metadata for '${repo}' via 'gh': ${
        err instanceof Error ? err.message : String(err)
      }`
    );
  }
}

/**
 * Applies repository setting changes using `gh repo edit`.
 */
export function applyRepoSettings(
  repo: string,
  diff: SettingsDiff,
  desired: RepoSettingsFile,
  execCmd: ExecFunction = defaultExec
): string[] {
  if (!diff.hasChanges) {
    return [];
  }

  const args: string[] = ["repo", "edit", repo];

  if (diff.description) {
    args.push("--description", desired.repository.description);
  }

  if (diff.homepage) {
    args.push("--homepage", desired.repository.homepage ?? "");
  }

  if (diff.topicsToAdd.length > 0) {
    args.push("--add-topic", diff.topicsToAdd.join(","));
  }

  if (diff.topicsToRemove.length > 0) {
    args.push("--remove-topic", diff.topicsToRemove.join(","));
  }

  try {
    execCmd("gh", args);
  } catch (err) {
    throw new Error(
      `Failed to update repository settings for '${repo}' via 'gh repo edit': ${
        err instanceof Error ? err.message : String(err)
      }`
    );
  }

  return args;
}
