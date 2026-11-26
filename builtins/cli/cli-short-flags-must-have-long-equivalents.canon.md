---
inspect:
  - diff
tags:
  - cli
---
Every single-character short flag supported by a CLI command MUST serve as an alias for a self-descriptive long-form flag; standalone short flags lacking long equivalents are forbidden.

Precedent: POSIX Utility Syntax Guideline 3; GNU `getopt_long` standards.

Rationale: Long-form flags ensure automated scripts and CI workflows remain self-documenting, while short aliases preserve human typing speed.
