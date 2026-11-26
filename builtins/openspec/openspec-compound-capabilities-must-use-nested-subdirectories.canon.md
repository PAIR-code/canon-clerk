---
triggers:
  - "openspec/changes/**/specs/**"
  - "openspec/specs/**"
exists:
  - "openspec/**"
inspect:
  - diff
tags:
  - openspec
  - architecture
---
OpenSpec capabilities governing extensible or multi-faceted domain areas MUST organize sub-capabilities into nested subdirectories rather than accumulating unbounded requirements into a single flat `spec.md`.

Precedent: Progressive Disclosure (Nielsen); Modular Decomposition (Parnas).

Rationale: Decomposing compound capabilities into cohesive subdirectories prevents monolithic documents, minimizes merge collisions, and optimizes context window retrieval for AI coding assistants.
