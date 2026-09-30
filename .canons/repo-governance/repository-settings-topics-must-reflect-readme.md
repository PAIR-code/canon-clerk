---
triggers:
  - "README.md"
references:
  - ".github/settings.yml"
inspect:
  - diff
tags:
  - repo-governance
  - repo-settings
---
Pull requests introducing, removing, or renaming core capabilities, tools, or architectural domains in `README.md` MUST update `repository.topics` in `.github/settings.yml` to reflect the active taxonomy.
