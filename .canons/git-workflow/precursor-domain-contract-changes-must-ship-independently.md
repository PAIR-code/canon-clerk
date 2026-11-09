---
inspect:
  - diff
tags:
  - git-workflow
---
Upstream domain behavioral or API contract changes discovered while developing downstream features or adapters MUST be extracted, reviewed, and merged in an independent pull request.

Exception: A pull request MAY decompose an existing module into smaller submodules IFF the extracted code's behavior and public API contracts remain unchanged.

Rationale: Following Kent Beck and Martin Fowler’s preparatory refactoring principle ("Make the change easy, then make the easy change") and *Software Engineering at Google* (Ch. 19, "Small Changes"), isolating upstream behavioral and contract changes from downstream work maintains atomic review units and preserves clean git bisectability.

**Remediation:** Pause WIP to surface the upstream contract gap to the operator (Andon cord).
