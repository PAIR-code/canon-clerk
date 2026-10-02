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
  - test-design
  - code-quality
---
Test suites verifying multiple distinct functional capabilities, operational facets, or execution contexts MUST partition test cases into topical sub-suites (such as BDD `describe` blocks, nested test classes, or subtest suites) aligned with those specific subdomains.

Exception: Test suites exercising a singular, focused behavioral contract or evaluating a pure, single-purpose utility MAY maintain a single unnested test suite IFF all test cases verify variations of that same contract.

Rationale: In accordance with *Software Engineering at Google* (Ch. 12: Unit Testing) and BDD specification patterns, structuring tests into topical contexts delineates failure domains, improves test runner diagnostics, and provides navigable executable documentation.
