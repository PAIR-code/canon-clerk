---
triggers:
  - "**/.canons/**/*.md"
inspect:
  - diff
tags:
  - canon-authoring
---
When a canon invariant codifies an established domain practice, formal specification, or industry consensus, the canon MUST explicitly cite the governing standard or precedent.

Rationale: Explicit standard citations (e.g. RFCs, POSIX, IEEE, SemVer, clig.dev) serve as high-affinity latent anchors for evaluator models, grounding edge-case evaluation in established consensus without verbose prose.

**Guidance:** Identify the governing specification or industry precedent (such as RFC 2119, POSIX.1-2017, The Twelve-Factor App, or clig.dev) and cite it directly within the invariant statement or `Rationale` directive.
