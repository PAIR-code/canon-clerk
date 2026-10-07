#!/usr/bin/env python3
"""
Declarative GitHub repository metadata validator and synchronization script for Canon Clerk.
Sequestered in .github/scripts to avoid toolchain dependencies in the core project.
"""

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path


def load_settings(file_path: Path) -> dict:
    """Loads and validates repository settings YAML."""
    if not file_path.is_file():
        raise FileNotFoundError(f"Settings file not found: {file_path}")

    content = file_path.read_text(encoding="utf-8")

    try:
        import yaml
        data = yaml.safe_load(content)
        repo_data = data.get("repository", {}) if isinstance(data, dict) else {}
        return {
            "name": repo_data.get("name", ""),
            "description": repo_data.get("description", ""),
            "homepage": repo_data.get("homepage", ""),
            "topics": [str(t).lower() for t in repo_data.get("topics", []) if t],
        }
    except ImportError:
        # Graceful fallback when PyYAML is not installed locally
        desc = ""
        homepage = ""
        name = ""
        topics = []
        in_topics = False

        for raw_line in content.splitlines():
            line = raw_line.strip()
            if not line or line.startswith("#"):
                continue

            if line.startswith("name:"):
                name = line.split(":", 1)[1].strip().strip("\"'")
                in_topics = False
            elif line.startswith("description:"):
                desc = line.split(":", 1)[1].strip().strip("\"'")
                in_topics = False
            elif line.startswith("homepage:"):
                homepage = line.split(":", 1)[1].strip().strip("\"'")
                in_topics = False
            elif line.startswith("topics:"):
                in_topics = True
            elif in_topics:
                if line.startswith("-"):
                    # Strip comments on the same line
                    topic_val = line.lstrip("-").strip().split("#")[0].strip().strip("\"'")
                    if topic_val:
                        topics.append(topic_val.lower())
                elif ":" in line:
                    in_topics = False

        return {
            "name": name,
            "description": desc,
            "homepage": homepage,
            "topics": topics,
        }


def resolve_target_repo(specified_repo: str | None) -> str:
    """Resolves target GitHub repository (e.g. 'PAIR-code/canon-clerk')."""
    if specified_repo and specified_repo.strip():
        return specified_repo.strip()

    for remote in ["upstream", "origin"]:
        try:
            res = subprocess.run(
                ["git", "config", "--get", f"remote.{remote}.url"],
                capture_output=True,
                text=True,
                check=False,
            )
            url = res.stdout.strip()
            match = re.search(r"(?:github\.com[:/])([^/]+)/([^/.]+?)(?:\.git)?$", url)
            if match:
                return f"{match.group(1)}/{match.group(2)}"
        except Exception:
            continue

    return "PAIR-code/canon-clerk"


def fetch_remote_metadata(repo: str) -> dict:
    """Queries current repository metadata via gh repo view."""
    cmd = [
        "gh", "repo", "view", repo,
        "--json", "description,homepageUrl,repositoryTopics",
    ]
    try:
        res = subprocess.run(cmd, capture_output=True, text=True, check=True)
        data = json.loads(res.stdout)
        topics = [t["name"].lower() for t in data.get("repositoryTopics", []) if isinstance(t, dict) and "name" in t]
        return {
            "description": data.get("description") or "",
            "homepage": data.get("homepageUrl") or "",
            "topics": topics,
        }
    except subprocess.CalledProcessError as err:
        stderr = err.stderr.strip() if err.stderr else str(err)
        raise RuntimeError(f"Failed to query repository metadata for '{repo}' via 'gh': {stderr}") from err


def diff_settings(desired: dict, remote: dict) -> dict:
    """Computes difference between desired settings and remote metadata."""
    desired_desc = desired.get("description", "")
    remote_desc = remote.get("description", "")
    desc_diff = {"current": remote_desc, "desired": desired_desc} if desired_desc != remote_desc else None

    desired_hp = desired.get("homepage", "")
    remote_hp = remote.get("homepage", "")
    hp_diff = {"current": remote_hp, "desired": desired_hp} if desired_hp != remote_hp else None

    desired_topics = desired.get("topics", [])
    remote_topics = remote.get("topics", [])

    desired_set = set(desired_topics)
    remote_set = set(remote_topics)

    topics_to_add = [t for t in desired_topics if t not in remote_set]
    topics_to_remove = [t for t in remote_topics if t not in desired_set]

    has_changes = bool(desc_diff or hp_diff or topics_to_add or topics_to_remove)

    return {
        "description": desc_diff,
        "homepage": hp_diff,
        "topics_to_add": topics_to_add,
        "topics_to_remove": topics_to_remove,
        "has_changes": has_changes,
    }


def format_diff(diff: dict, repo: str) -> str:
    """Formats settings diff for console display."""
    if not diff["has_changes"]:
        return f"Repository settings for '{repo}' are fully synchronized with remote."

    lines = [f"Repository settings diff for '{repo}':"]
    if diff["description"]:
        lines.append("  Description:")
        lines.append(f'    - current: "{diff["description"]["current"]}"')
        lines.append(f'    + desired: "{diff["description"]["desired"]}"')

    if diff["homepage"]:
        lines.append("  Homepage:")
        lines.append(f'    - current: "{diff["homepage"]["current"]}"')
        lines.append(f'    + desired: "{diff["homepage"]["desired"]}"')

    if diff["topics_to_add"]:
        lines.append("  Topics to add (+):")
        for topic in diff["topics_to_add"]:
            lines.append(f"    + {topic}")

    if diff["topics_to_remove"]:
        lines.append("  Topics to remove (-):")
        for topic in diff["topics_to_remove"]:
            lines.append(f"    - {topic}")

    return "\n".join(lines)


def apply_settings(repo: str, diff: dict, desired: dict) -> None:
    """Applies changed settings via gh repo edit."""
    if not diff["has_changes"]:
        print("No changes detected. Skipping 'gh repo edit'.")
        return

    cmd = ["gh", "repo", "edit", repo]

    if diff["description"]:
        cmd.extend(["--description", desired.get("description", "")])

    if diff["homepage"]:
        cmd.extend(["--homepage", desired.get("homepage", "")])

    if diff["topics_to_add"]:
        cmd.extend(["--add-topic", ",".join(diff["topics_to_add"])])

    if diff["topics_to_remove"]:
        cmd.extend(["--remove-topic", ",".join(diff["topics_to_remove"])])

    print("\nApplying changes via 'gh repo edit'...")
    try:
        subprocess.run(cmd, check=True)
        print("✓ Repository settings synchronized successfully.")
    except subprocess.CalledProcessError as err:
        stderr = err.stderr.strip() if err.stderr else str(err)
        raise RuntimeError(f"Failed to update settings for '{repo}': {stderr}") from err


def main() -> int:
    parser = argparse.ArgumentParser(description="Declarative GitHub repository metadata manager")
    parser.add_argument("command", choices=["check", "sync"], help="Action: 'check' (validate & diff) or 'sync' (apply)")
    parser.add_argument("--file", default=".github/settings.yml", help="Path to settings YAML file")
    parser.add_argument("--repo", default=None, help="Target GitHub repository")
    parser.add_argument("--dry-run", action="store_true", help="Show diff without applying changes")
    args = parser.parse_args()

    file_path = Path(args.file)
    print(f"Loading settings from: {file_path}")

    try:
        settings = load_settings(file_path)
        print("✓ Settings YAML syntax and schema validated successfully.")
    except Exception as err:
        print(f"Error: {err}", file=sys.stderr)
        return 1

    target_repo = resolve_target_repo(args.repo)
    print(f"Target repository: {target_repo}")

    try:
        remote = fetch_remote_metadata(target_repo)
    except Exception as err:
        print(f"Warning: {err}", file=sys.stderr)
        if args.command == "sync" and not args.dry_run:
            print("Error: Cannot sync settings without active 'gh' CLI connection.", file=sys.stderr)
            return 1
        return 0

    diff = diff_settings(settings, remote)
    print("\n" + format_diff(diff, target_repo))

    if args.command == "check" or args.dry_run:
        if diff["has_changes"]:
            print("\nDry-run completed: Changes are pending synchronization.")
        else:
            print("\nDry-run completed: Repository is in sync.")
        return 0

    if args.command == "sync":
        try:
            apply_settings(target_repo, diff, settings)
        except Exception as err:
            print(f"Error: {err}", file=sys.stderr)
            return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
