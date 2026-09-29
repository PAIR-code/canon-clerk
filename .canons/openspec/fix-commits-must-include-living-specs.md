---
triggers:
  - "**/src/**"
inspect:
  - diff
  - pr_title
  - commit_messages
tags:
  - openspec
  - conventional-commits
---
Pull requests repairing user-facing defects (`fix`) MUST be accompanied by a new or updated specification scenario in `openspec/specs/`.

Exception: A `fix` pull request MAY omit changes to `openspec/specs/` IFF the pull request description explicitly cites an existing requirement and scenario in `openspec/specs/` whose text already unambiguously specifies the expected behavior (demonstrating that the defect was an unfaithful code implementation rather than a specification gap).

Rationale: Under Conventional Commits 1.0.0 (SemVer PATCH) and Specification by Example (SBE), user-facing defects indicate unhandled boundary conditions or gaps in specification scenarios. Codifying the corrected behavior as a distinct OpenSpec scenario (`#### Scenario:`) anchors the fix as a regression guardrail and preserves living contract truth.

**Remediation:** Add a new `#### Scenario:` under the affected requirement in `openspec/specs/<capability>/spec.md` codifying the failing input and expected output as a concrete regression guardrail.
