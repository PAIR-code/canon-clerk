---
triggers:
  - "**/*.canon.md"
inspect:
  - diff
tags:
  - canon-authoring
---
Canon invariants MUST demarcate applicability positively by naming the governed construct and precondition, rather than enumerating orthogonal constructs via negative parentheticals (`excluding ...`) inside the invariant clause.

Rationale: In autoregressive relevance screening, listing orthogonal constructs inside an invariant seeds lexical token overlap with unrelated prompts and diffs (keyword priming), pulling orthogonal canons toward the decision boundary and inducing run-to-run flips.

**Remediation:** Move genuine carve-outs for cases falling inside the governed construct's scope into a formal `Exception` directive (`MAY ... IFF ...`), and rely on positive subject specificity to exclude orthogonal constructs.
