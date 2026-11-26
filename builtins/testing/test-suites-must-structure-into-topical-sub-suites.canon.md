---
triggers:
  - "**/*.test.*"
  - "**/*.spec.*"
  - "**/*_test.*"
  - "**/test_*.py"
inspect:
  - diff
tags:
  - testing
  - code-quality
---
Test suites verifying multiple distinct functional capabilities, operational facets, or execution contexts MUST partition test cases into topical sub-suites aligned with those specific subdomains.

Exception: Test suites exercising a singular, focused behavioral contract or evaluating a pure, single-purpose utility MAY maintain a single unnested test suite IFF all test cases verify variations of that same contract.

Precedent: *Software Engineering at Google* (Ch. 12, "Unit Testing"); BDD specification patterns.

Rationale: Structuring tests into topical contexts delineates failure domains, improves test runner diagnostics, and provides navigable executable documentation.
