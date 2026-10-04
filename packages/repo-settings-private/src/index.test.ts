import { describe, it, expect } from "vitest";
import { parseRepoSettings } from "./schema.js";
import { diffRepoSettings, formatDiff } from "./diff.js";
import { applyRepoSettings, resolveTargetRepo } from "./gh.js";
import type { RepoSettingsFile, RemoteRepoMetadata } from "./types.js";

describe("@canon-clerk/repo-settings-private", () => {
  describe("schema parsing & validation", () => {
    it("successfully parses valid settings YAML", () => {
      const yaml = `
repository:
  name: "canon-clerk"
  description: "Semantic linter for project canons."
  homepage: "https://github.com/PAIR-code/canon-clerk"
  topics:
    - canon
    - spec
    - semantic-linter
`;
      const result = parseRepoSettings(yaml);
      expect(result.repository.name).toBe("canon-clerk");
      expect(result.repository.description).toBe("Semantic linter for project canons.");
      expect(result.repository.homepage).toBe("https://github.com/PAIR-code/canon-clerk");
      expect(result.repository.topics).toEqual(["canon", "spec", "semantic-linter"]);
    });

    it("parses minimal valid configuration", () => {
      const yaml = `
repository:
  description: "Minimal description."
`;
      const result = parseRepoSettings(yaml);
      expect(result.repository.description).toBe("Minimal description.");
      expect(result.repository.homepage).toBeUndefined();
      expect(result.repository.topics).toBeUndefined();
    });

    it("throws on malformed YAML syntax", () => {
      expect(() => parseRepoSettings("repository: [unclosed")).toThrow(/Failed to parse settings YAML/);
    });

    it("throws when repository section is missing", () => {
      expect(() => parseRepoSettings("other_key: true")).toThrow(/missing 'repository' section/);
    });

    it("throws on invalid topic formatting", () => {
      const yaml = `
repository:
  description: "Valid"
  topics:
    - "Invalid Topic With Spaces"
`;
      expect(() => parseRepoSettings(yaml)).toThrow(/Invalid GitHub topic/);
    });
  });

  describe("diff calculation", () => {
    const local: RepoSettingsFile = {
      repository: {
        description: "New description",
        homepage: "https://github.com/PAIR-code/canon-clerk",
        topics: ["canon", "spec", "semantic-linter", "rule-packs"],
      },
    };

    const remote: RemoteRepoMetadata = {
      description: "Old description",
      homepageUrl: "",
      repositoryTopics: [{ name: "canon" }, { name: "spec" }, { name: "legacy-topic" }],
    };

    it("calculates accurate diff between local and remote", () => {
      const diff = diffRepoSettings(local, remote);
      expect(diff.hasChanges).toBe(true);
      expect(diff.description).toEqual({
        current: "Old description",
        desired: "New description",
      });
      expect(diff.homepage).toEqual({
        current: "",
        desired: "https://github.com/PAIR-code/canon-clerk",
      });
      expect(diff.topicsToAdd).toEqual(["semantic-linter", "rule-packs"]);
      expect(diff.topicsToRemove).toEqual(["legacy-topic"]);
    });

    it("returns hasChanges: false when settings match exactly", () => {
      const inSyncRemote: RemoteRepoMetadata = {
        description: "New description",
        homepageUrl: "https://github.com/PAIR-code/canon-clerk",
        repositoryTopics: [
          { name: "canon" },
          { name: "spec" },
          { name: "semantic-linter" },
          { name: "rule-packs" },
        ],
      };

      const diff = diffRepoSettings(local, inSyncRemote);
      expect(diff.hasChanges).toBe(false);
      expect(diff.description).toBeUndefined();
      expect(diff.homepage).toBeUndefined();
      expect(diff.topicsToAdd).toHaveLength(0);
      expect(diff.topicsToRemove).toHaveLength(0);
    });

    it("formats human-readable diff message", () => {
      const diff = diffRepoSettings(local, remote);
      const output = formatDiff(diff, "PAIR-code/canon-clerk");
      expect(output).toContain("Description:");
      expect(output).toContain("- current: \"Old description\"");
      expect(output).toContain("+ desired: \"New description\"");
      expect(output).toContain("Topics to add (+):");
      expect(output).toContain("+ semantic-linter");
      expect(output).toContain("Topics to remove (-):");
      expect(output).toContain("- legacy-topic");
    });
  });

  describe("gh CLI execution", () => {
    it("assembles correct gh repo edit arguments", () => {
      const local: RepoSettingsFile = {
        repository: {
          description: "New description",
          homepage: "https://example.com",
          topics: ["canon", "spec"],
        },
      };

      const diff = {
        description: { current: "Old", desired: "New description" },
        homepage: { current: "", desired: "https://example.com" },
        topicsToAdd: ["spec"],
        topicsToRemove: ["legacy"],
        hasChanges: true,
      };

      let executedCommand = "";
      let executedArgs: string[] = [];

      const mockExec = (command: string, args: string[]) => {
        executedCommand = command;
        executedArgs = args;
        return "";
      };

      const args = applyRepoSettings("PAIR-code/canon-clerk", diff, local, mockExec);
      expect(executedCommand).toBe("gh");
      expect(args).toEqual([
        "repo",
        "edit",
        "PAIR-code/canon-clerk",
        "--description",
        "New description",
        "--homepage",
        "https://example.com",
        "--add-topic",
        "spec",
        "--remove-topic",
        "legacy",
      ]);
      expect(executedArgs).toEqual(args);
    });

    it("returns empty args and skips exec when hasChanges is false", () => {
      const local: RepoSettingsFile = {
        repository: { description: "Same" },
      };

      const diff = {
        topicsToAdd: [],
        topicsToRemove: [],
        hasChanges: false,
      };

      let called = false;
      const mockExec = () => {
        called = true;
        return "";
      };

      const args = applyRepoSettings("PAIR-code/canon-clerk", diff, local, mockExec);
      expect(args).toEqual([]);
      expect(called).toBe(false);
    });

    it("resolves target repo from override or git remote", () => {
      expect(resolveTargetRepo("custom/repo")).toBe("custom/repo");

      const mockExec = (_cmd: string, args: string[]) => {
        if (args.includes("remote.upstream.url")) {
          return "git@github.com:PAIR-code/canon-clerk.git\n";
        }
        throw new Error("not found");
      };

      expect(resolveTargetRepo(undefined, mockExec)).toBe("PAIR-code/canon-clerk");
    });
  });
});
