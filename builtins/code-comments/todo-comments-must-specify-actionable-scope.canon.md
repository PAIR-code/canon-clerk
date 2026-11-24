---
triggers:
  - "**"
inspect:
  - diff
tags:
  - code-comments
---
`TODO` and `FIXME` comments MUST state the specific defect or deferred work clearly enough to be acted upon without external tribal knowledge.

Rationale: Per Google Style Guides ("TODO Comments"), vacuous placeholders such as `// TODO: clean this up` accumulate indefinitely because future maintainers cannot determine what completion requires.
