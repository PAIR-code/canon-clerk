---
inspect:
  - diff
tags:
  - cli-ergonomics
---
CLI command help screens (`--help`) MUST include at least one realistic, copy-pasteable runnable usage example demonstrating common flag configurations alongside argument definitions.

Rationale: Flag definitions document individual option syntax in isolation but fail to illustrate composable invocation workflows, forcing developers and AI agents to guess valid command combinations.

**Guidance:** Include an `Examples:` block within the command help metadata displaying representative, functional command invocations for primary use cases.
