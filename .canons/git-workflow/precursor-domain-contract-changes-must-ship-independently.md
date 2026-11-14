---
inspect:
  - diff
  - pr_body
  - linked_issues
tags:
  - git-workflow
---
Behavioral or API contract changes to upstream domain modules that are not explicitly sanctioned by the motivating issue MUST be extracted, reviewed, and merged in an independent precursor pull request rather than bundled into downstream feature or adapter changes.

Exception: A pull request MAY decompose an existing module into smaller submodules IFF the extracted code's behavior and public API contracts remain unchanged.

Rationale: Following Kent Beck and Martin Fowler’s preparatory refactoring principle ("Make the change easy, then make the easy change") and *Software Engineering at Google* (Ch. 19, "Small Changes"), isolating upstream behavioral and contract changes from downstream work maintains atomic review units and preserves clean git bisectability.

**Remediation:** Pause in-flight feature work (Andon cord) to extract the upstream contract change into a standalone precursor pull request, or execute an upstream chase to sanction the contract expansion in the motivating issue and pull request description.
