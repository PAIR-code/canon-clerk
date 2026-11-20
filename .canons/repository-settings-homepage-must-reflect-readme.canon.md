---
triggers:
  - "README.md"
references:
  - ".github/settings.yml"
inspect:
  - diff
tags:
  - internal
  - repo-governance
---
Pull requests modifying the canonical documentation URL or public site link in `README.md` MUST update `repository.homepage` in `.github/settings.yml` to match.
