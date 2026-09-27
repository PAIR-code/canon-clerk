---
paths:
  - "**/.canons/**/*.md"
inspect:
  - diff
tags:
  - reference
  - canon-authoring
---
Canon content MUST be succinct, stating each invariant rule, engineering rationale, and remediation instruction with maximal information density and zero repetition.

Rationale: Extraneous prose, conversational filler, and repeated assertions consume evaluator prompt tokens and dilute model reasoning without conveying additional semantic signal.

**Guidance:** Strip conversational preamble, tutorial narrative, and repeated assertions. Ensure every sentence contributes a distinct, necessary signal for an AI agent reader.
