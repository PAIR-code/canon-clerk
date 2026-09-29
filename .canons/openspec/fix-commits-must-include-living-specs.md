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
Pull requests repairing user-facing defects (`fix`) MUST be accompanied by a new or updated specification scenario in `openspec/specs/`, unless the existing specification already unambiguously specified the behavior.

Rationale: Under Conventional Commits 1.0.0 (SemVer PATCH) and Specification by Example (SBE), user-facing defects indicate unhandled boundary conditions or gaps in specification scenarios. Codifying the corrected behavior as a distinct OpenSpec scenario (`#### Scenario:`) anchors the fix as a regression guardrail and preserves living contract truth.

**Guidance:** Add a new `#### Scenario:` under the affected requirement in `openspec/specs/<capability>/spec.md` reflecting the defect being repaired. If the existing specification already explicitly and unambiguously specified the correct behavior (i.e. the defect was a pure code deviation from the spec), cite the governing requirement and scenario in the pull request description.
