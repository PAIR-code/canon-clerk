---
triggers:
  - "README.md"
inspect:
  - diff
tags:
  - readme-authoring
---
The repository `README.md` MUST shunt detailed architectural execution mechanics, pipeline cascades, and internal dataflow specifications to dedicated documents under `docs/` or `specs/`.

Precedent: Diátaxis documentation framework.

Rationale: The repository landing page is the storefront for adopters and contributors; inlining complex architectural specifications increases cognitive load and duplicates authoritative system documentation maintained under `docs/` or `specs/`.
