---
triggers:
  - "**"
tags:
  - security
  - secrets-management
---
Plaintext API keys, authentication tokens, and private credentials MUST NOT be written to or stored within the repository workspace directory tree, even when excluded from version control via `.gitignore`. Repositories and local development workflows MUST source secrets exclusively from host process environment variables or user-level configuration directories located outside the workspace envelope.

Rationale: In accordance with OWASP Secrets Management guidelines and AI coding assistant security standards, tool-using AI coding agents operate within the workspace with arbitrary file-reading capabilities; keeping plaintext credentials inside the repository root risks unintentional exfiltration or exposure in tool traces and model contexts.
