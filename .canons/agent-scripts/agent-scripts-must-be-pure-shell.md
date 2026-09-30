---
triggers:
  - ".agents/skills/**"
tags:
  - agent-scripts
---
Agent skill scripts MUST be written purely in POSIX shell syntax. Scripts MUST NOT embed or evaluate inlined programs in secondary interpreted languages (such as Python, Ruby, Perl, or Node.js heredocs or inline execution flags).

Rationale: Embedding secondary language runtimes inside shell scripts introduces hidden interpreter dependencies, sidesteps language-specific linting, and obscures failure modes in non-interactive agent environments.
