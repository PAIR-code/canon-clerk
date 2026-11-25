---
triggers:
  - "docs/**/*.md"
  - "specs/**/*.md"
inspect:
  - diff
tags:
  - documentation
---
Prose in living specification documents MUST state the system's current contract in timeless present tense rather than narrating historical migrations, changelogs, or temporal rollout phases.

Exception: A living specification MAY reference retired or deprecated interfaces IFF documenting an active runtime rejection, compatibility contract, or formal deprecation policy currently enforced by the system, OR inside an explicitly historical ADR.

Precedent: Living Documentation (Martraire); Diátaxis Reference Documentation.

Rationale: Temporal narration rots as the system evolves and forces human readers and AI agents to reconcile superseded behavior against the active contract.
