---
triggers:
  - ".agents/skills/**"
tags:
  - agent-scripts
---
Agent skill scripts MUST serve as lightweight plumbing conduits that directly stream underlying tool outputs to stdout/stderr, and MUST NOT synthesize custom reporting dashboards or re-format standard tool output.

Rationale: AI coding assistants read standard CLI output natively; re-parsing tool outputs into bespoke dashboards adds latency, creates maintenance overhead, and corrodes agent confidence in the output.
