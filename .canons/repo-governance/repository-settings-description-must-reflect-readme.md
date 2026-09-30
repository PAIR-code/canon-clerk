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
Pull requests modifying the project purpose, tagline, or core summary in `README.md` MUST update `repository.description` in `.github/settings.yml` to reflect the updated positioning.
