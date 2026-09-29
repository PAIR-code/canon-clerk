---
inspect:
  - diff
tags:
  - cli-ergonomics
---
CLI commands that accept collections of paths, files, or query targets MUST support receiving piped stream input from `stdin` (signaled via `-` or when `stdin` is piped and arguments are omitted).

Rationale: Per POSIX utility conventions and the Unix composition principle, supporting standard input streams enables seamless piping from tools like `git diff` without encountering OS argument length limits (`ARG_MAX`).

**Remediation:** Detect when `-` is supplied as an operand or when `!process.stdin.isTTY`, reading newline-delimited stream items into the collection before executing query resolution.
