---
triggers:
  - "**/*.canon.md"
inspect:
  - diff
tags:
  - canon-authoring
---
Canon invariants MUST demarcate applicability positively by naming the governed construct and precondition, rather than enumerating orthogonal constructs via negative parentheticals (`excluding ...`) inside the invariant clause.

Exception: Genuine carve-outs for specific cases that would otherwise fall inside the governed construct's scope MAY be declared using a formal `Exception` directive (`MAY ... IFF ...`).

Rationale: In autoregressive relevance screening, listing orthogonal constructs inside an invariant seeds lexical token overlap with unrelated prompts and diffs (keyword priming), pulling orthogonal canons toward the decision boundary and inducing run-to-run flips.
