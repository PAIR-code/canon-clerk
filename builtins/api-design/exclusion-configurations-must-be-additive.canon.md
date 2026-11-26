---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - api-design
  - configuration
---
APIs and configuration interfaces that accept custom path ignore or exclusion patterns MUST treat user-provided patterns as additive on top of default noise exclusions rather than replacing defaults.

Exception: APIs and configuration interfaces MAY bypass default exclusions IFF an explicit opt-out option or flag (such as `defaultIgnores: false` or `--no-default-ignores`) is invoked.

Precedent: Industry tooling conventions (ESLint flat config, Prettier, Ripgrep).

Rationale: Replacing default exclusions (such as `node_modules`, `.git`, or build outputs) when a caller specifies a custom pattern silently triggers noisy, high-latency traversal of deep dependency trees.
