---
triggers:
  - ".agents/skills/**"
inspect:
  - diff
  - pr_body
tags:
  - internal
  - agent-scripts
---
Agent skill scripts under `.agents/skills/**` MUST be written purely in POSIX shell syntax. Scripts MUST NOT embed or evaluate inlined programs in secondary interpreted languages (such as Python, Ruby, Perl, or Node.js heredocs or inline execution flags).

Rationale: Embedding secondary language runtimes inside shell scripts introduces hidden interpreter dependencies, sidesteps language-specific linting, and obscures failure modes in non-interactive agent environments.

**Remediation:** Replace embedded language scripts with standard POSIX utilities (such as `sed` or `awk`) and sanctioned tool capabilities (such as `gh --jq`) composed via shell pipelines, or implement complex logic as domain features in workspace packages rather than agent skill scripts.
