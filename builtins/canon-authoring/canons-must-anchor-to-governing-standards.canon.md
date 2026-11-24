---
triggers:
  - "**/*.canon.md"
inspect:
  - diff
tags:
  - canon-authoring
---
When a canon invariant codifies an established domain practice, formal specification, or industry consensus, the canon MUST explicitly cite the governing standard or precedent in a `Precedent:` directive.

Exception: Canons codifying purely project-internal conventions or proprietary workflows MAY omit formal standard citations IFF no external industry specification or standard precedent exists for the subject matter.

Rationale: Explicit standard citations (e.g. RFCs, POSIX, IEEE, SemVer, clig.dev) serve as high-affinity latent anchors for evaluator models, grounding edge-case evaluation in established consensus without verbose prose.
