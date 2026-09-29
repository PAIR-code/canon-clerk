---
triggers:
  - "README.md"
inspect:
  - diff
tags:
  - readme-authoring
---
The repository `README.md` MUST shunt detailed architectural execution mechanics, pipeline cascades, and internal dataflow specifications to dedicated documents under `docs/` or `specs/`.

Rationale: The repository landing page is the storefront for adopters and contributors; inlining complex architectural specifications increases cognitive load and duplicates authoritative system documentation maintained under `docs/` or `specs/` (per the Diátaxis documentation framework).

**Remediation:** Move detailed architecture diagrams, pipeline cascades, or execution mechanics to a dedicated document under `docs/` or `specs/`, leaving only a concise summary and link in `README.md`.
