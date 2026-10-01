---
inspect:
  - diff
tags:
  - git-workflow
---
Foundational domain or core engine refactors discovered while developing presentation adapters (such as CLI commands or CI workflows) MUST be extracted, reviewed, and merged in an independent pull request before landing the presentation layer.

Rationale: Following Kent Beck and Martin Fowler’s preparatory refactoring principle ("Make the change easy, then make the easy change") and *Software Engineering at Google* (Ch. 19, "Small Changes"), isolating structural domain refactorings from presentation additions maintains atomic review units and preserves clean git bisectability.
