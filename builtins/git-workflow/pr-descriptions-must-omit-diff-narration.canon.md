---
inspect:
  - pr_body
  - diff
tags:
  - git-workflow
---
Pull request descriptions MUST NOT enumerate file-by-file or symbol-by-symbol change inventories that restate the code diff.

Rationale: Following *Software Engineering at Google* (Ch. 19, "Small Changes") and standard code-review communication practices, the diff itself is the authoritative record of mechanical edits; restating it in prose dilutes reviewer attention from design intent, trade-offs, and behavioral impact.
