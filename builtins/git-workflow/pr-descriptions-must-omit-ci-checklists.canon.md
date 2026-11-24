---
inspect:
  - pr_body
tags:
  - git-workflow
---
Pull request descriptions MUST NOT include verification summaries or checklists restating automated CI gate results (such as formatter, linter, license, or unit-test pass status).

Rationale: Automated CI status checks provide authoritative, machine-verified pass/fail state on every commit; manual prose claims in PR descriptions become stale after subsequent pushes and add zero conditional signal.
