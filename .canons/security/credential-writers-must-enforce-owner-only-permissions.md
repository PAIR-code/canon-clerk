---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - security
  - filesystem
---
Source code routines that create or persist file-based credentials to the host filesystem MUST explicitly specify POSIX `0o600` mode (`-rw-------`, readable and writable solely by the owner).

Exception: Operating systems lacking native POSIX permission semantics (such as Windows) MAY omit octal mode enforcement IFF file operations fail safe and preserve platform access control.

Rationale: Per POSIX.1-2017 security guidelines and CIS benchmarks, writing credential files with ambient or default umask permissions risks exposing sensitive secrets to other users and processes on shared hosts.
