---
triggers:
  - "packages/**/package.json"
inspect:
  - diff
tags:
  - repo-governance
  - monorepo
---
Non-published monorepo packages serving internal tooling, settings, or test harnesses MUST append `-private` to their package manifest name and workspace directory.

Rationale: Citing Angular's monorepo workspace taxonomy (e.g. `@angular/dev-infra-private`) and npm package privacy conventions, explicit `-private` suffixing distinguishes internal test and configuration harnesses from public consumption libraries across directory trees, import specifiers, and release filtering.
