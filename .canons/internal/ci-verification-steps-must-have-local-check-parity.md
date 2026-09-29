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
Pull requests adding, removing, or modifying verification gates (linters, typecheckers, test suites, distribution builds, etc.) across `.github/workflows/ci.yml` and the `package.json` `check` script MUST maintain semantic parity between the two.

**Exception:** A verification step MAY be declared in only one surface IFF an inline explanatory comment in that file explicitly documents why local or remote execution is inapplicable (e.g. runner-specific virtualization or credential requirements).

Rationale: In accordance with Martin Fowler's Continuous Integration principles (automated local/CI build parity) and Continuous Delivery commit-stage practices (Humble & Farley), local verification scripts provide the primary shift-left mechanism for contributors and AI agents; discrepancies between local checks and continuous integration lead to preventable review cycle churn.

**Guidance:** Mirror verification steps added to `.github/workflows/ci.yml` into the root `package.json` `check` pipeline using `npm run`, or annotate one-sided gates with an inline explanatory comment.
