---
references:
  - "**/package.json"
tags:
  - semver
  - versioning
---
Packages with major version zero (`0.y.z`) MUST NOT retain deprecated aliases, forwarding shims, or transitional compatibility layers when renaming internal domain concepts; refactors MUST atomically update all call sites and excise retired terminology.

Rationale: Semantic Versioning 2.0.0 Clause 4 specifies that initial development (`0.y.z`) is inherently unstable and subject to rapid iteration. Preserving compatibility shims for unreleased internal constructs introduces dead code, splits developer lexicon, and complicates architectural maintenance without user benefit.
