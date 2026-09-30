#!/usr/bin/env node
import { resolve, dirname } from "node:path";
import { existsSync } from "node:fs";
import { loadRepoSettingsFile } from "./schema.js";
import { diffRepoSettings, formatDiff } from "./diff.js";
import { resolveTargetRepo, fetchRemoteMetadata, applyRepoSettings } from "./gh.js";

function findSettingsFile(specifiedPath: string): string {
  if (specifiedPath !== ".github/settings.yml") {
    return resolve(process.cwd(), specifiedPath);
  }

  let dir = process.cwd();
  while (true) {
    const candidate = resolve(dir, ".github/settings.yml");
    if (existsSync(candidate)) {
      return candidate;
    }
    const parent = dirname(dir);
    if (parent === dir) {
      break;
    }
    dir = parent;
  }

  return resolve(process.cwd(), specifiedPath);
}

function printUsage(): void {
  console.log(`
Usage: repo-settings <command> [options]

Commands:
  check   Validate settings YAML syntax and show planned diff against GitHub
  sync    Apply declarative settings to GitHub via 'gh repo edit'

Options:
  --file <path>   Path to settings YAML file (default: .github/settings.yml)
  --repo <repo>   Target GitHub repository (default: auto-detected or PAIR-code/canon-clerk)
  --dry-run       Show diff without applying changes (applicable to 'sync')
  -h, --help      Show this help message
`);
}

export async function runCli(argv: string[] = process.argv.slice(2)): Promise<number> {
  let command = "";
  let filePath = ".github/settings.yml";
  let repo: string | undefined;
  let dryRun = false;

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "check" || arg === "sync") {
      command = arg;
    } else if (arg === "--file" && i + 1 < argv.length) {
      const nextArg = argv[++i];
      if (nextArg !== undefined) {
        filePath = nextArg;
      }
    } else if (arg === "--repo" && i + 1 < argv.length) {
      const nextArg = argv[++i];
      if (nextArg !== undefined) {
        repo = nextArg;
      }
    } else if (arg === "--dry-run") {
      dryRun = true;
    } else if (arg === "-h" || arg === "--help") {
      printUsage();
      return 0;
    } else {
      console.error(`Unknown argument: ${arg}`);
      printUsage();
      return 1;
    }
  }

  if (!command) {
    console.error("Error: A command ('check' or 'sync') must be specified.");
    printUsage();
    return 1;
  }

  const resolvedPath = findSettingsFile(filePath);
  console.log(`Loading settings from: ${resolvedPath}`);

  let settings;
  try {
    settings = loadRepoSettingsFile(resolvedPath);
    console.log("✓ Settings YAML syntax and schema validated successfully.");
  } catch (err) {
    console.error(`Error: ${(err as Error).message}`);
    return 1;
  }

  const targetRepo = resolveTargetRepo(repo);
  console.log(`Target repository: ${targetRepo}`);

  let remoteMetadata;
  try {
    remoteMetadata = fetchRemoteMetadata(targetRepo);
  } catch (err) {
    console.warn(`Warning: Could not fetch remote metadata via 'gh' (${(err as Error).message}).`);
    if (command === "sync" && !dryRun) {
      console.error("Error: Cannot sync settings without active 'gh' CLI connection.");
      return 1;
    }
    return 0;
  }

  const diff = diffRepoSettings(settings, remoteMetadata);
  console.log("\n" + formatDiff(diff, targetRepo));

  if (command === "check" || dryRun) {
    if (diff.hasChanges) {
      console.log("\nDry-run completed: Changes are pending synchronization.");
    } else {
      console.log("\nDry-run completed: Repository is in sync.");
    }
    return 0;
  }

  if (command === "sync") {
    if (!diff.hasChanges) {
      console.log("No changes detected. Skipping 'gh repo edit'.");
      return 0;
    }

    console.log("\nApplying changes via 'gh repo edit'...");
    try {
      const executedArgs = applyRepoSettings(targetRepo, diff, settings);
      console.log(`Executed: gh ${executedArgs.join(" ")}`);
      console.log("✓ Repository settings synchronized successfully.");
    } catch (err) {
      console.error(`Error: ${(err as Error).message}`);
      console.error(
        "\nRemediation: Updating repository metadata (description, homepage, topics) on GitHub requires administrative permissions on the repository." +
        "\n  • For local runs: Ensure 'gh auth status' is logged into an account with repository admin permissions." +
        "\n  • For CI runs: Ensure the 'REPO_SETTINGS_TOKEN' secret is configured with 'Administration: Read and write' permissions." +
        "\n  • Refer to packages/repo-settings/README.md for token creation and secret setup instructions."
      );
      return 1;
    }
  }

  return 0;
}

// If invoked directly from CLI
if (process.argv[1] && (process.argv[1].endsWith("/cli.js") || process.argv[1].endsWith("/cli.ts"))) {
  runCli().then((code) => {
    if (code !== 0) {
      process.exit(code);
    }
  });
}
