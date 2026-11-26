---
triggers:
  - "**/*.canon.md"
inspect:
  - diff
tags:
  - canon-authoring
---
Canon invariants, whether stated in the body or derived from the filename or heading when the body omits an invariant clause, MUST state a single primary obligation rather than embedding alternative compliance paths in the invariant clause.

Exception: A canon invariant MAY use `or` IFF it joins equivalent phrasings or sub-parts of a single cohesive mechanism or governed construct rather than distinct compliance paths.

Precedent: Canon Format Specification (§5).

Rationale: `Exception` directives provide structured logical `OR` with verifiable `IFF` conditions; embedding branching compliance paths inside the invariant clause conflates primary policy with exemptions and blurs audit verdicts.

**Remediation:** State one primary invariant and move each alternate compliance path into an `Exception` directive with an `IFF` condition, or split disjoint governed constructs into separate canon files.
