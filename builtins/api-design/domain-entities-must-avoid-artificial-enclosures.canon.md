---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - api-design
---
Domain entities (DDD) representing a singular system resource MUST expose primary attributes directly at the top level rather than partitioning them into artificial nested wrapper objects.

Exception: Domain entities MAY encapsulate raw wire records or provenance metadata in nested fields IFF primary domain attributes remain exposed at the top level.

Precedent: Domain-Driven Design (Evans); The Zen of Python ("flat is better than nested").

Rationale: Artificial wrapper records create accessor friction and obscure domain properties that naturally describe the entity itself.

**Remediation:** Flatten sub-object interfaces into the top-level domain entity by having the primary entity type extend or intersect the constituent interfaces.
