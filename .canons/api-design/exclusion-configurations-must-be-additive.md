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

Exception: APIs that provide explicit, dedicated options or flags designed specifically to disable default exclusions (such as `defaultIgnores: false` or `--no-default-ignores`) MAY bypass default rules.

Rationale: In accordance with industry tooling conventions (ESLint flat config, Prettier, Ripgrep), replacing default exclusions (such as `node_modules`, `.git`, or build outputs) when a caller specifies a custom pattern silently triggers noisy, high-latency traversal of deep dependency trees.
