---
triggers:
  - "**/src/**/index.ts"
  - "**/src/**/version.ts"
references:
  - "**/package.json"
inspect:
  - diff
tags:
  - npm
  - package-manifest
  - versioning
  - release-engineering
---
In npm packages, exported runtime versions and schema identifiers MUST source dynamically from the package manifest (`package.json`) rather than declaring duplicate hardcoded string literals.

Rationale: Following the Single Source of Truth (SSOT) principle (Hunt & Thomas) and the npm manifest specification, duplicating version literals in source code causes version drift during automated release bumps.

**Remediation:** Import `version` directly from the local `package.json` (via static JSON module imports inlined at compile time) or expose it via build tool define flags, and export that dynamic binding.
