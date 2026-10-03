---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - security
  - filesystem
---
Source code routines that read or load file-based credentials from the host filesystem MUST inspect file mode permissions and refuse to read credentials (failing fast or ignoring insecure files) if permissions allow group or world read access on POSIX platforms.

Exception: Operating systems lacking native POSIX permission semantics (such as Windows) MAY bypass octal mode verification IFF file operations fail safe and preserve platform access control.

Rationale: Following the OpenSSH and GnuPG security precedent, credential defense-in-depth requires active verification at the consumption boundary; silently reading secrets from world-readable files permits privilege escalation and credential exfiltration on multi-user systems.
