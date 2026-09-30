/**
 * Declarative repository settings configuration structure matching .github/settings.yml.
 */
export interface RepoSettingsConfig {
  name?: string | undefined;
  description: string;
  homepage?: string | undefined;
  topics?: string[] | undefined;
}

/**
 * Root structure of .github/settings.yml file.
 */
export interface RepoSettingsFile {
  repository: RepoSettingsConfig;
}

/**
 * Remote repository metadata as returned by GitHub CLI `gh repo view --json description,homepageUrl,repositoryTopics`.
 */
export interface RemoteRepoMetadata {
  description: string;
  homepageUrl: string;
  repositoryTopics: { name: string }[];
}

/**
 * Computed difference between local settings and remote GitHub metadata.
 */
export interface SettingsDiff {
  description?: {
    current: string;
    desired: string;
  } | undefined;
  homepage?: {
    current: string;
    desired: string;
  } | undefined;
  topicsToAdd: string[];
  topicsToRemove: string[];
  hasChanges: boolean;
}
