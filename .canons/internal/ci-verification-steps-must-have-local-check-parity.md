---
triggers:
  - ".github/workflows/ci.yml"
  - "package.json"
inspect:
  - diff
tags:
  - internal
  - repo-governance
---
Pull requests adding, removing, or modifying verification gates (linters, typecheckers, test suites, distribution builds, etc.) across `.github/workflows/ci.yml` and the `package.json` `check` script MUST maintain semantic parity between the two. If a verification gate is strictly one-sided, it MUST be accompanied by an explanatory comment justifying the exception.

Rationale: Local verification scripts provide the primary shift-left mechanism for contributors and AI agents; discrepancies between local checks and continuous integration lead to preventable review cycle churn.

**Guidance:** Ensure every verification command executed in `ci.yml` has a direct local equivalent chained into the root `npm run check` script (or explicitly document in the file why the step is strictly one-sided).
