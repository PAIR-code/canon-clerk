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
Unit tests MUST NOT make change-detector assertions against incidental release metadata, hardcoded version literals, build hashes, or generated timestamps.

Exception: Backward-compatibility regression suites MAY assert against frozen historical version constants IFF they validate wire-format or serialization compatibility against legacy test fixtures.

Precedent: *Software Engineering at Google* (Ch. 12, "Unit Testing").

Rationale: Asserting against incidental metadata detects no behavioral regressions while guaranteeing synthetic CI failures during routine release bumps.

**Remediation:** Replace literal metadata assertions with structural pattern assertions (e.g. SemVer regex) or relational assertions against the source manifest.
