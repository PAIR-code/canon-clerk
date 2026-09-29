---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - api-design
  - data-contracts
---
Domain entities (DDD) representing a singular system resource MUST expose primary attributes directly at the top level rather than partitioning them into artificial nested wrapper objects.

Exception: Raw or unparsed source representations (such as unparsed payload text, raw wire records, or provenance metadata) intended for low-level debugging or audit logging MAY remain encapsulated.

Rationale: In accordance with Domain-Driven Design (Evans) and the Zen of Python ("flat is better than nested"), artificial wrapper records create accessor friction and obscure domain properties that naturally describe the entity itself.

**Remediation:** Flatten sub-object interfaces into the top-level domain entity by having the primary entity type extend or intersect the constituent interfaces.
