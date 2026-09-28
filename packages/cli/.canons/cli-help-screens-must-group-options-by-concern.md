---
inspect:
  - diff
tags:
  - cli-ergonomics
---
CLI commands that expose more than six configuration options MUST organize their `--help` screen flags into categorized sections (such as Execution, Output, and Credentials) rather than displaying a flat list.

Rationale: Flat flag lists force users and AI assistants to scan dozens of unrelated options to find relevant settings, obscuring primary workflow flags behind advanced configuration options.

**Guidance:** Configure the CLI argument parser with option groups or categories (e.g. `Input Options`, `Execution Options`, `Output Options`) to provide structured progressive disclosure in `--help` output.
