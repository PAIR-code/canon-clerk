import { parse } from "yaml";
import { readFileSync } from "node:fs";
import type { RepoSettingsFile } from "./types.js";

const TOPIC_REGEX = /^[a-z0-9]([a-z0-9-]{0,48}[a-z0-9])?$/;

/**
 * Parses and validates raw YAML content as a RepoSettingsFile.
 */
export function parseRepoSettings(yamlContent: string): RepoSettingsFile {
  let doc: unknown;
  try {
    doc = parse(yamlContent);
  } catch (err) {
    throw new Error(`Failed to parse settings YAML: ${err instanceof Error ? err.message : String(err)}`);
  }

  if (!doc || typeof doc !== "object") {
    throw new Error("Invalid settings YAML: root must be an object containing a 'repository' field.");
  }

  const { repository } = doc as Record<string, unknown>;
  if (!repository || typeof repository !== "object") {
    throw new Error("Invalid settings YAML: missing 'repository' section.");
  }

  const repo = repository as Record<string, unknown>;

  if (typeof repo.description !== "string") {
    throw new Error("Invalid settings YAML: 'repository.description' must be a string.");
  }

  if (repo.homepage !== undefined && typeof repo.homepage !== "string") {
    throw new Error("Invalid settings YAML: 'repository.homepage' must be a string if provided.");
  }

  if (repo.name !== undefined && typeof repo.name !== "string") {
    throw new Error("Invalid settings YAML: 'repository.name' must be a string if provided.");
  }

  let topics: string[] | undefined;
  if (repo.topics !== undefined) {
    if (!Array.isArray(repo.topics)) {
      throw new Error("Invalid settings YAML: 'repository.topics' must be an array of strings.");
    }

    topics = [];
    for (const item of repo.topics) {
      if (typeof item !== "string") {
        throw new Error(`Invalid settings YAML: topic item must be a string, found: ${JSON.stringify(item)}`);
      }
      const trimmed = item.trim().toLowerCase();
      if (!TOPIC_REGEX.test(trimmed)) {
        throw new Error(
          `Invalid GitHub topic '${item}': topics must be lowercase alphanumeric characters or hyphens, up to 50 characters.`
        );
      }
      topics.push(trimmed);
    }
  }

  return {
    repository: {
      name: repo.name as string | undefined,
      description: repo.description.trim(),
      homepage: typeof repo.homepage === "string" ? repo.homepage.trim() : undefined,
      topics,
    },
  };
}

/**
 * Reads and parses a repository settings YAML file from disk.
 */
export function loadRepoSettingsFile(filePath: string): RepoSettingsFile {
  const content = readFileSync(filePath, "utf-8");
  return parseRepoSettings(content);
}
