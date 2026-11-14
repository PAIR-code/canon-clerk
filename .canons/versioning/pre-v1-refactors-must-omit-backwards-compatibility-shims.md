---
inspect:
  - diff
references:
  - "**/Cargo.toml"
  - "**/package.json"
  - "**/pyproject.toml"
tags:
  - semver
  - versioning
---
Refactors that rename or replace internal domain concepts in major-version-zero (`0.y.z`) packages MUST atomically update all call sites and excise retired terminology without retaining deprecated aliases, forwarding shims, or transitional compatibility layers.

Rationale: Semantic Versioning 2.0.0 Clause 4 specifies that initial development (`0.y.z`) is inherently unstable and subject to rapid iteration. Preserving compatibility shims for unreleased internal constructs introduces dead code, splits developer lexicon, and complicates architectural maintenance without user benefit.
