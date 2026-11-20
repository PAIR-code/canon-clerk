---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - configuration
  - api-design
---
String specifiers selecting models or backends across multiple providers MUST require an explicit `<provider>:<name>` prefix rather than inferring the provider from bare identifier substrings.

Rationale: Grounded in RFC 3986 URI scheme syntax, prefix heuristics silently misroute requests when local gateways, proxies, or competing vendors serve identically named models.
