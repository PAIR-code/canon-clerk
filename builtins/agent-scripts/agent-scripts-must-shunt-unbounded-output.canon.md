---
triggers:
  - ".agents/skills/**"
tags:
  - agent-scripts
---
Command invocations with potentially unbounded output in agent skill scripts MUST enforce an explicit output-size ceiling and shunt payloads exceeding it to a temporary file.

Rationale: Assistant subshell runners impose strict stdout truncation caps (empirically ~8KB on current harnesses); shunting large payloads to temporary files preserves full diagnostics for paginated inspection.
