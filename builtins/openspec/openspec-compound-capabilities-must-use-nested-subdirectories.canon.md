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

Rationale: In accordance with Progressive Disclosure (Nielsen) and modular specification architecture (Parnas), decomposing compound capabilities into cohesive subdirectories prevents monolithic documents, minimizes merge collisions, and optimizes context window retrieval for AI coding assistants.
