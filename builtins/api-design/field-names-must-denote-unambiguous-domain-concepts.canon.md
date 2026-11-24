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
Public schema, configuration, and API field names MUST denote specific domain concepts and structural semantics on a cold read rather than generic or overloaded placeholders (such as `rules`, `items`, `data`, `config`, `options`, or `metadata`) whose meaning requires inspecting implementation code.

Exception: Generic container field names (such as `items` or `data`) MAY be used IFF mandated by an external wire standard or envelope specification (such as JSON:API, OpenAPI pagination envelopes, or GraphQL connections).

Rationale: Following Domain-Driven Design Ubiquitous Language (Evans) and the Google API Design Guide (AIP-122, AIP-140), self-describing domain identifiers eliminate ambiguity for cold-read human reviewers and AI agents operating without full codebase context.
