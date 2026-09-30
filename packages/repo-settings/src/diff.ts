import type { RepoSettingsFile, RemoteRepoMetadata, SettingsDiff } from "./types.js";

/**
 * Computes difference between local desired settings and remote GitHub repository metadata.
 */
export function diffRepoSettings(
  local: RepoSettingsFile,
  remote: RemoteRepoMetadata
): SettingsDiff {
  const desired = local.repository;
  const desiredDesc = desired.description;
  const currentDesc = remote.description ?? "";

  let descriptionDiff: { current: string; desired: string } | undefined;
  if (currentDesc !== desiredDesc) {
    descriptionDiff = {
      current: currentDesc,
      desired: desiredDesc,
    };
  }

  const desiredHomepage = desired.homepage ?? "";
  const currentHomepage = remote.homepageUrl ?? "";

  let homepageDiff: { current: string; desired: string } | undefined;
  if (currentHomepage !== desiredHomepage) {
    homepageDiff = {
      current: currentHomepage,
      desired: desiredHomepage,
    };
  }

  const currentTopics = (remote.repositoryTopics ?? []).map((t) => t.name.toLowerCase());
  const desiredTopics = (desired.topics ?? []).map((t) => t.toLowerCase());

  const currentTopicSet = new Set(currentTopics);
  const desiredTopicSet = new Set(desiredTopics);

  const topicsToAdd = desiredTopics.filter((t) => !currentTopicSet.has(t));
  const topicsToRemove = currentTopics.filter((t) => !desiredTopicSet.has(t));

  const hasChanges = Boolean(
    descriptionDiff ||
    homepageDiff ||
    topicsToAdd.length > 0 ||
    topicsToRemove.length > 0
  );

  return {
    description: descriptionDiff,
    homepage: homepageDiff,
    topicsToAdd,
    topicsToRemove,
    hasChanges,
  };
}

/**
 * Formats a SettingsDiff into human-readable plain text output.
 */
export function formatDiff(diff: SettingsDiff, repo: string): string {
  if (!diff.hasChanges) {
    return `Repository settings for '${repo}' are fully synchronized with remote.`;
  }

  const lines: string[] = [`Repository settings diff for '${repo}':`];

  if (diff.description) {
    lines.push(`  Description:`);
    lines.push(`    - current: "${diff.description.current}"`);
    lines.push(`    + desired: "${diff.description.desired}"`);
  }

  if (diff.homepage) {
    lines.push(`  Homepage:`);
    lines.push(`    - current: "${diff.homepage.current}"`);
    lines.push(`    + desired: "${diff.homepage.desired}"`);
  }

  if (diff.topicsToAdd.length > 0) {
    lines.push(`  Topics to add (+):`);
    for (const topic of diff.topicsToAdd) {
      lines.push(`    + ${topic}`);
    }
  }

  if (diff.topicsToRemove.length > 0) {
    lines.push(`  Topics to remove (-):`);
    for (const topic of diff.topicsToRemove) {
      lines.push(`    - ${topic}`);
    }
  }

  return lines.join("\n");
}
