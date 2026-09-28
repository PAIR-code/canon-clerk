---
triggers:
  - "README.md"
inspect:
  - diff
tags:
  - readme-authoring
---
The repository `README.md` MUST lead with a concise statement of the project's core purpose ("what") and value proposition or problem space ("why").

Rationale: Per the Inverted Pyramid principle and the Diátaxis documentation framework, visitors and AI coding assistants rely on the opening lines of landing documentation for rapid orientation; burying the project's purpose beneath installation commands, badge walls, or implementation details impairs comprehension and causes high cognitive bounce rates.

**Guidance:** Place a high-signal declarative lead immediately beneath the top-level heading in `README.md`, articulating what the project does and why it exists (e.g. via an elevator statement or concise problem/solution pair) before proceeding to secondary sections.
