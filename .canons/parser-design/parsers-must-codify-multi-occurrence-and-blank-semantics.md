---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - parser-design
  - data-integrity
---
Document parsers MUST explicitly define and verify behavior for all four data states: missing fields, blank/whitespace-only content, repeated directives, and wrongly-typed inputs.

Rationale: Adhering to Postel's Robustness Principle (RFC 1122) and boundary-value analysis, parsers without explicit semantics for missing, blank, repeated, and mistyped inputs degrade unpredictably or throw unhandled runtime errors.

**Remediation:** In the specification and unit tests, explicitly codify derivation fallbacks for missing fields, strip whitespace-only payloads to `undefined` or empty collections, define collection vs concatenation semantics for repeated directives, and degrade mistyped fields gracefully while preserving raw input.
