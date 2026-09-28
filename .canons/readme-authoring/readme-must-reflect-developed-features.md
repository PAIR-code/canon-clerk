---
governs:
  - "**/*"
references:
  - "README.md"
inspect:
  - diff
  - pr_title
tags:
  - readme-authoring
---
Pull requests introducing new user-facing functionality (`type: feat`) MUST update `README.md` to document the new capability, its invocation, or its configuration.

Rationale: Features merged into default branches without landing documentation updates remain invisible to adopters and create discrepancies between software capabilities and documentation.

**Guidance:** Update `README.md` to reflect the newly introduced feature or API in the appropriate section.
