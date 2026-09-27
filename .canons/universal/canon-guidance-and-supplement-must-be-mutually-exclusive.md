---
paths:
  - "**/.canons/**/*.md"
inspect:
  - diff
tags:
  - universal
  - canon-authoring
---
A canon MUST NOT declare both `Guidance` and `Supplement` directives. An invariant violation either mandates author remediation (`Guidance`, resolving to `fail`) or triggers automated clerk synthesis (`Supplement`, resolving to `warn`), but never both.

Rationale: Guidance and Supplement establish conflicting target actors (contributor vs. clerk AI) and incompatible CI verdicts (blocking fail vs. non-blocking warn); co-declaration introduces ambiguity into the evaluation contract.

**Guidance:** Determine whether the invariant requires manual author action or automated clerk synthesis. If author remediation is required, retain Guidance and remove Supplement. If automated synthesis is intended, retain Supplement and remove Guidance.
