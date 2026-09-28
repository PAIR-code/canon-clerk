---
inspect:
  - diff
tags:
  - cli-ergonomics
---
CLI commands MUST detect non-interactive or automated environments (`!isTTY`, `CI=true`, or `NO_COLOR=1`) and automatically disable interactive prompts, ANSI color escapes, and animated terminal spinners.

Rationale: Interactive prompts cause unattended CI pipelines to hang indefinitely until timeout, while raw ANSI escape sequences and animated spinners pollute build logs with indecipherable noise.

**Guidance:** Check terminal TTY status and standard environment variables (`CI`, `NO_COLOR`) before initiating prompts or ANSI rendering, terminating with an actionable non-zero exit code if required parameters are missing in non-interactive mode.
