---
inspect:
  - diff
tags:
  - cli-ergonomics
---
By default, CLI commands that report domain resources or configuration entities in structured format (`--json`) MUST emit the fully resolved schema with all computed and default attributes populated, rather than sparse input snippets or unparsed files, unless an explicit opt-out flag (such as `--raw`) is requested.

Rationale: In accordance with standard API and resource projection conventions (e.g. Kubernetes, OpenAPI), downstream automation requires complete, predictable schemas without having to reconstruct defaulted or derived fields.

**Guidance:** Ensure structured output serializers hydrate default and derived attributes before emitting JSON, reserving unparsed source text strictly for explicit user-requested flags like `--raw`.
