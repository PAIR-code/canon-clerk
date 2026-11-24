---
triggers:
  - "**/*.canon.md"
inspect:
  - diff
tags:
  - canon-authoring
---
Canon invariant clauses MUST express quantitative bounds conceptually rather than hardcoding numeric thresholds.

Exception: A canon invariant clause MAY specify a numeric threshold IFF the bound is mandated by an external governing standard, protocol specification, or explicit platform contract (e.g., a cryptographic key-length floor, a protocol payload limit, or a fixed forge API pagination cap).

Rationale: Numeric caps vary across tools and time, coupling semantic rules to brittle heuristics that belong in advisory `Rationale`/`Remediation` notes, project config, or deterministic linters.
