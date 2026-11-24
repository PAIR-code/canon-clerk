---
triggers:
  - "**/src/**"
  - "docs/**/*.md"
  - "specs/**/*.md"
inspect:
  - diff
tags:
  - api-design
  - configuration
---
Public schema, configuration, and API field names MUST self-descriptively denote specific domain concepts and structural semantics from the schema signature alone rather than generic or overloaded placeholders whose meaning requires inspecting implementation code.

Exception: Generic container field names (such as `items` or `data`) MAY be used IFF mandated by an external wire standard or envelope specification (such as JSON:API, OpenAPI pagination envelopes, or GraphQL connections).

Precedent: Domain-Driven Design Ubiquitous Language (Evans); Google API Design Guide (AIP-122, AIP-140).
