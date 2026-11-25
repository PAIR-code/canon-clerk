---
triggers:
  - "docs/**/*.md"
  - "specs/**/*.md"
inspect:
  - diff
tags:
  - documentation
---
Empirical latency or throughput measurements dependent on specific hardware, models, or external providers in living specification documents MUST NOT appear in normative specification prose or tables.

Exception: A living specification MAY include concrete latency, throughput, or token figures IFF they define an enforced architectural budget, timeout, or SLA contract, OR appear inside a clearly labeled illustrative example or benchmark section.

Precedent: ISO/IEC Directives Part 2 (Normative vs. Informative Content); Living Documentation (Martraire).

Rationale: Unlabeled empirical measurements silently drift across sibling specifications as models and infrastructure change, misleading readers and AI agents into treating point-in-time observations as normative contracts.
